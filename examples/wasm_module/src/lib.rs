#[no_mangle]
extern "C" fn alloc(len: u32) -> u32 {
    let len = len as usize;
    let layout = core::alloc::Layout::from_size_align(len, 8).unwrap();
    let ptr = unsafe { std::alloc::alloc(layout) };
    return (ptr as usize) as u32;
}
