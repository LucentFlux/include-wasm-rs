use include_wasm_rs::build_wasm;

fn main() {
    let module = build_wasm!("./wasm_module");

    println!("wasm bytes: {module:?}");
}
