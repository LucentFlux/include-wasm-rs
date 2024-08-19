use include_wasm_rs::build_wasm;

fn main() {
    let module = build_wasm! {
        path: "../examples/wasm_module",
        features: [bulk_memory],
        env: Env {
            MY_ENV_VAR: 12,
        },
        release: true,
    };

    println!("wasm bytes: {module:?}");
}
