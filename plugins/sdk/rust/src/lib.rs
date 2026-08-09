use std::alloc::{self, Layout};
use std::mem;

/// 分配 host 可读的 guest 内存；返回的指针必须由宿主通过 `myc_free` 释放。
/// Allocates host-readable guest memory; the returned pointer must be freed by
/// the host using `myc_free`.
#[no_mangle]
pub extern "C" fn myc_alloc(size: i32) -> i32 {
    let size = size.max(0) as usize;
    if size == 0 {
        return 0;
    }
    let layout = Layout::from_size_align(size, 1).expect("valid layout");
    let pointer = unsafe { alloc::alloc(layout) };
    if pointer.is_null() {
        return 0;
    }
    pointer as i32
}

/// 释放由 `myc_alloc` 返回的 guest 内存；size 必须与分配时一致。
/// Frees memory returned by `myc_alloc`; `size` must match the allocation.
#[no_mangle]
pub extern "C" fn myc_free(pointer: i32, size: i32) {
    let pointer = pointer as *mut u8;
    let size = size.max(0) as usize;
    if pointer.is_null() || size == 0 {
        return;
    }
    let layout = Layout::from_size_align(size, 1).expect("valid layout");
    unsafe { alloc::dealloc(pointer, layout) };
}

#[no_mangle]
pub extern "C" fn myc_run(_input_pointer: i32, _input_length: i32) -> i64 {
    let output = br#"{"runtime":"rust","status":"ok"}"#.to_vec().into_boxed_slice();
    let length = output.len() as u64;
    let pointer = output.as_ptr() as u64;
    mem::forget(output);
    ((pointer & 0xffff_ffff) << 32 | length) as i64
}
