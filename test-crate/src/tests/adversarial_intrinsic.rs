use std::ptr;
use std::cell::RefCell;

unsafe fn intrinsic_leaker(value: &u64, sink: &u64) {
    let sink = sink as *const u64;
    ptr::copy(value as *const u64, sink as *mut u64, 1);
}
