//! Provides a macro for including a Rust project as Wasm bytecode,
//! by compiling it at build time of the invoking module.

#![feature(mutex_unpoison)]
#![feature(proc_macro_span)]

use std::{fmt::Display, path::PathBuf, process::Command, sync::Mutex};

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse::ParseStream, parse_macro_input, spanned::Spanned};

#[derive(Default)]
struct TargetFeatures {
    atomics: bool,
    bulk_memory: bool,
    mutable_globals: bool,
}

impl TargetFeatures {
    fn from_list_of_exprs(
        elems: syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>,
    ) -> syn::parse::Result<Self> {
        let mut res = Self::default();

        for elem in elems {
            let span = elem.span();
            let name = match elem {
                syn::Expr::Path(ident)
                    if ident.attrs.is_empty()
                        && ident.qself.is_none()
                        && ident.path.leading_colon.is_none()
                        && ident.path.segments.len() == 1
                        && ident.path.segments[0].arguments.is_empty() =>
                {
                    ident.path.segments[0].ident.to_string()
                }
                _ => {
                    return Err(syn::Error::new(
                        span,
                        "expected a single token giving a feature",
                    ))
                }
            };

            match name.as_str() {
                "atomics" => res.atomics = true,
                "bulk_memory" => res.bulk_memory = true,
                "mutable_globals" => res.mutable_globals = true,
                _ => return Err(syn::Error::new(span, "unknown feature")),
            }
        }

        return Ok(res);
    }
}

#[derive(Default)]
struct Args {
    module_dir: PathBuf,
    features: TargetFeatures,
    env_vars: Vec<(String, String)>,
    release: bool,
}

impl syn::parse::Parse for Args {
    fn parse(input: ParseStream) -> syn::parse::Result<Self> {
        // Just a string gives a path, with default options
        if input.peek(syn::LitStr) {
            let path = input.parse::<syn::LitStr>()?;
            return Ok(Self {
                module_dir: PathBuf::from(path.value()),
                ..Self::default()
            });
        }

        // Else we expect a json-like dict of options
        let mut res = Self::default();

        let dict =
            syn::punctuated::Punctuated::<syn::FieldValue, syn::Token![,]>::parse_terminated(
                input,
            )?;
        for value in dict {
            if !value.attrs.is_empty() {
                return Err(syn::Error::new(value.attrs[0].span(), "unexpected element"));
            }
            let name = match &value.member {
                syn::Member::Named(name) => name.to_string(),
                syn::Member::Unnamed(unnamed) => unnamed.index.to_string(),
            };

            // Parse value depending on key
            match name.as_str() {
                "path" => {
                    // String as PathBuf
                    res.module_dir = match value.expr {
                        syn::Expr::Lit(syn::ExprLit {
                            attrs,
                            lit: syn::Lit::Str(path),
                        }) if attrs.is_empty() => PathBuf::from(path.value()),
                        _ => return Err(syn::Error::new(value.expr.span(), "expected string")),
                    };
                }
                "release" => {
                    // Boolean
                    res.release = match value.expr {
                        syn::Expr::Lit(syn::ExprLit {
                            attrs,
                            lit: syn::Lit::Bool(release),
                        }) if attrs.is_empty() => release.value,
                        _ => return Err(syn::Error::new(value.expr.span(), "expected boolean")),
                    };
                }
                "features" => {
                    // Array of identifiers
                    match value.expr {
                        syn::Expr::Array(syn::ExprArray {
                            attrs,
                            bracket_token: _,
                            elems,
                        }) if attrs.is_empty() => {
                            res.features = TargetFeatures::from_list_of_exprs(elems)?
                        }
                        _ => return Err(syn::Error::new(value.expr.span(), "expected boolean")),
                    };
                }
                "env" => {
                    // Dictionary of key value pairs
                    match value.expr {
                        syn::Expr::Struct(syn::ExprStruct {
                            attrs,
                            qself: None,
                            path:
                                syn::Path {
                                    leading_colon: None,
                                    segments,
                                },
                            brace_token: _,
                            fields,
                            dot2_token: None,
                            rest: None,
                        }) if attrs.is_empty()
                            && segments.len() == 1
                            && segments[0].arguments.is_empty()
                            && segments[0].ident.to_string() == "Env" =>
                        {
                            for field in fields {
                                let span = field.span();
                                if !field.attrs.is_empty() || !field.colon_token.is_some() {
                                    return Err(syn::Error::new(span, "expected key value pair"));
                                }

                                let env_name = match &field.member {
                                    syn::Member::Named(name) => name.to_string(),
                                    _ => {
                                        return Err(syn::Error::new(
                                            span,
                                            "expected env variable name",
                                        ))
                                    }
                                };

                                let mut expr = &field.expr;
                                while let syn::Expr::Group(syn::ExprGroup {
                                    attrs,
                                    group_token: _,
                                    expr: inner_expr,
                                }) = expr
                                {
                                    if !attrs.is_empty() {
                                        return Err(syn::Error::new(
                                            attrs[0].span(),
                                            "expected a string, int, float or bool",
                                        ));
                                    }

                                    expr = inner_expr;
                                }

                                let env_val = match expr {
                                    syn::Expr::Lit(syn::ExprLit { attrs, lit })
                                        if attrs.is_empty() =>
                                    {
                                        match lit {
                                            syn::Lit::Str(v) => v.value(),
                                            syn::Lit::Int(i) => i.to_string(),
                                            syn::Lit::Float(f) => f.to_string(),
                                            syn::Lit::Bool(b) => b.value.to_string(),
                                            _ => {
                                                return Err(syn::Error::new(
                                                    lit.span(),
                                                    format!("expected a string, int, float or bool, found literal `{}`", lit.into_token_stream().to_string()),
                                                ))
                                            }
                                        }
                                    }
                                    _ => {
                                        return Err(syn::Error::new(
                                            field.expr.span(),
                                            format!("expected a string, int, float or bool, found `{}`", field.expr.into_token_stream().to_string()),
                                        ))
                                    }
                                };

                                res.env_vars.push((env_name, env_val));
                            }
                        }
                        _ => {
                            return Err(syn::Error::new(
                                value.expr.span(),
                                "expected key value pairs",
                            ))
                        }
                    }
                }
                option => {
                    return Err(syn::Error::new(
                        value.member.span(),
                        format!("unknown option `{}`", option),
                    ))
                }
            }
        }

        return Ok(res);
    }
}

impl Display for TargetFeatures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.atomics {
            write!(f, "+atomics,")?
        }
        if self.bulk_memory {
            write!(f, "+bulk-memory,")?
        }
        if self.mutable_globals {
            write!(f, "+mutable-globals,")?
        }

        Ok(())
    }
}

/// Only allow one build job at a time, in case we are building one module many times.
static GLOBAL_LOCK: Mutex<()> = Mutex::new(());

/// Builds a cargo project as a webassembly module, returning the bytes of the module produced.
fn do_build_wasm(args: &Args) -> Result<PathBuf, String> {
    let Args {
        module_dir,
        features,
        env_vars,
        release,
    } = args;

    // Acquire global lock
    let mut lock = GLOBAL_LOCK.lock();
    while let Err(_) = lock {
        GLOBAL_LOCK.clear_poison();
        lock = GLOBAL_LOCK.lock();
    }

    // Check target path points to a module
    let cargo_config = module_dir.join("Cargo.toml");
    if !cargo_config.is_file() {
        return Err(format!(
            "target directory `{}` does not contain a `Cargo.toml` file",
            module_dir.display()
        ));
    }
    match std::fs::read_to_string(cargo_config) {
        Ok(cfg) => {
            if cfg.contains("[workspace]\n") {
                return Err("provided directory points to a workspace, not a module".to_owned());
            }
        }
        Err(e) => return Err(format!("failed to read target `Cargo.toml`: {e}")),
    }

    // Build output path, taking env vars into account
    let mut target_dir = "target/".to_owned();
    for (key, val) in env_vars.iter() {
        target_dir += &format!("{}_{}", key, val);
    }

    // Construct build command
    let mut command = Command::new("cargo");

    // Treat `RUSTFLAGS` as special in env vars
    const RUSTFLAGS: &'static str = "RUSTFLAGS";
    let mut rustflags_value = format!("--cfg=web_sys_unstable_apis -C target-feature={features}");
    command.env(RUSTFLAGS, &rustflags_value);

    for (key, val) in env_vars.into_iter() {
        if key == RUSTFLAGS {
            rustflags_value += " ";
            rustflags_value += &val;
            command.env(RUSTFLAGS, &rustflags_value);
        } else {
            command.env(key, val);
        }
    }

    // Set args
    let mut args = vec![
        "+nightly",
        "build",
        "--target",
        "wasm32-unknown-unknown",
        "-Z",
        "build-std=panic_abort,std",
        "--target-dir",
        &target_dir,
    ];
    if *release {
        args.push("--release");
    }
    let out = command.args(args).current_dir(module_dir.clone()).output();

    match out {
        Ok(out) => {
            if !out.status.success() {
                return Err(format!(
                    "failed to build module `{}`: \n{}",
                    module_dir.display(),
                    String::from_utf8_lossy(&out.stderr).replace("\n", "\n\t")
                ));
            }
        }
        Err(e) => {
            return Err(format!(
                "failed to build module `{}`: {e}",
                module_dir.display()
            ))
        }
    }

    // Find output with glob
    let root_output = module_dir.join(target_dir).join("wasm32-unknown-unknown/");
    let glob = if *release {
        root_output.join("release/")
    } else {
        root_output.join("debug/")
    }
    .join("*.wasm");
    let mut glob_paths = glob::glob(
        &glob
            .as_os_str()
            .to_str()
            .expect("output path should be unicode compliant"),
    )
    .expect("glob should be valid");

    let output = match glob_paths.next() {
        Some(Ok(output)) => output,
        Some(Err(err)) => {
            return Err(format!(
                "failed to find output file matching `{glob:?}`: {err} - this is probably a bug",
            ))
        }
        None => {
            return Err(format!(
                "failed to find output file matching `{}` - this is probably a bug",
                glob.display()
            ))
        }
    };

    // Check only one output to avoid hidden bugs
    if let Some(Ok(_)) = glob_paths.next() {
        return Err(format!("multiple output files matching `{}` were found - this may be because you recently changed the name of your module; try deleting the folder `{}` and rebuilding", glob.display(), root_output.display()));
    }

    drop(lock);

    return Ok(output);
}

fn all_module_files(path: PathBuf) -> Vec<String> {
    let glob_paths = glob::glob(
        &path
            .as_os_str()
            .to_str()
            .expect("output path should be unicode compliant"),
    )
    .expect("glob should be valid");

    glob_paths
        .into_iter()
        .filter_map(|path| {
            let path = path.ok()?;
            if !path.is_file() {
                None
            } else {
                Some(path.to_string_lossy().to_string())
            }
        })
        .collect()
}

/// Invokes `cargo build` at compile time on another module, replacing this macro invocation
/// with the bytes contained in the output `.wasm` file.
///
/// # Usage
///
/// ```ignore
/// let module = build_wasm!("relative/path/to/module");
/// ```
///
/// # Arguments
///
/// This macro can take a number of additional arguments to control how the WebAssembly should be generated.
/// These options are passed to `cargo build`:
///
/// ```ignore
/// let module = build_wasm!{
///     path: "relative/path/to/module",
///     features: [
///         atomics, // Controls if the `atomics` proposal is enabled
///         bulk_memory, // Controls if the `bulk-memory` proposal is enabled
///         mutable_globals, // Controls if the `mutable-globals` proposal is enabled
///     ],
///     // Allows additional environment variables to be set while compiling the module.
///     env: Env {
///         FOO: "bar",
///         BAX: 7,
///     },
///     // Controls if the module should be built in debug or release mode.
///     release: true
/// };
/// ```
#[proc_macro]
pub fn build_wasm(args: TokenStream) -> TokenStream {
    let invocation_file = proc_macro::Span::call_site().source_file().path();
    let invocation_file = invocation_file
        .parent()
        .unwrap()
        .to_path_buf()
        .canonicalize()
        .unwrap();

    // Parse args
    let mut args = parse_macro_input!(args as Args);
    args.module_dir = invocation_file.join(args.module_dir);

    // Build
    let result = do_build_wasm(&args);

    // Output
    match result {
        Ok(bytes_path) => {
            let bytes_path = bytes_path.to_string_lossy().to_string();
            // Register rebuild on files changed
            let module_paths = all_module_files(args.module_dir);

            quote! {
                {
                    #(
                        let _ = include_str!(#module_paths);
                    )*
                    include_bytes!(#bytes_path) as &'static [u8]
                }
            }
        }
        Err(err) => quote! {
            {
                compile_error!(#err);
                const BS: &'static [u8] = &[0u8];
                BS
            }
        },
    }
    .into()
}
