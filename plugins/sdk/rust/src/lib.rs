use std::mem;

#[no_mangle]
pub extern "C" fn myc_alloc(size: i32) -> i32 {
    let mut buffer = Vec::<u8>::with_capacity(size.max(0) as usize);
    let pointer = buffer.as_mut_ptr();
    mem::forget(buffer);
    pointer as i32
}

#[no_mangle]
pub extern "C" fn myc_run(_input_pointer: i32, _input_length: i32) -> i64 {
    let output = br#"{"runtime":"rust","status":"ok"}"#.to_vec().into_boxed_slice();
    let length = output.len() as u64;
    let pointer = output.as_ptr() as u64;
    mem::forget(output);
    ((pointer & 0xffff_ffff) << 32 | length) as i64
}
