use include_wasm_rs::build_wasm;

macro_rules! module {
    ($path:expr) => {
        build_wasm! {
            path: $path,
            features: [bulk_memory],
            env: Env {
                MY_ENV_VAR: 12,
            },
            release: true,
        }
    };
}

fn main() {
    let module = module!("wasm_module");

    println!("wasm bytes: {module:?}");
}
