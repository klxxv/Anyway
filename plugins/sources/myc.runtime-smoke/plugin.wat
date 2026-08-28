(module
  (memory (export "memory") 1 2)
  (global $heap (mut i32) (i32.const 1024))
  (func (export "myc_alloc") (param $size i32) (result i32)
    global.get $heap
    global.get $heap
    local.get $size
    i32.add
    global.set $heap)
  (data (i32.const 16) "{\22runtime\22:\22ok\22,\22language\22:\22rust-cpp-abi\22}")
  (func (export "myc_run") (param i32 i32) (result i64)
    i64.const 68719476778))
