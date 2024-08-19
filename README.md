# Include Wasm-RS

[![crates.io](https://img.shields.io/crates/v/include-wasm-rs.svg)](https://crates.io/crates/include-wasm-rs)
[![docs.rs](https://img.shields.io/docsrs/include-wasm-rs)](https://docs.rs/include-wasm-rs/latest/include_wasm_rs/)
[![crates.io](https://img.shields.io/crates/l/include-wasm-rs.svg)](https://github.com/LucentFlux/include-wasm-rs/blob/main/LICENSE)

Builds a Rust WebAssembly module at compile time and returns the bytes.

# Example

```rust
let module = build_wasm!("relative/path/to/module");
```

This crate provides a wrapper around `cargo` to build and then include a WebAssembly module at compile time. This is intended for use in unit tests, where the target platform may not be able to invoke cargo itself, for example while using `MIRI` or when executing the compiled module on the Web.

# Toolchain

To use this crate, you must have the `wasm32-unknown-unknown` nightly toolchain installed, and have the `rust-src` component. You can install these with the following commands:

```bash
rustup target add wasm32-unknown-unknown --toolchain nightly
rustup component add rust-src
```

# Arguments

The build macro allows for an assortment of arguments to be passed to the build command:

```rust
let module = build_wasm!{
    path: "relative/path/to/module",
    features: [
        atomics, // Controls if the `atomics` proposal is enabled
        bulk_memory, // Controls if the `bulk-memory` proposal is enabled
        mutable_globals, // Controls if the `mutable-globals` proposal is enabled
    ],
    // Allows additional environment variables to be set while compiling the module.
    env: Env {
        FOO: "bar",
        BAX: 7,
    },
    // Controls if the module should be built in debug or release mode.
    release: true
};
```

# Features

If you're on nightly, the `proc_macro_span` feature will enable better call site location resolution.