use std::alloc::Global;

pub fn vec_new() {
    let v: Vec<usize> = Vec::new();
}

pub fn vec_with_capacity() {
    let v: Vec<usize> = Vec::with_capacity(32);
}

pub fn vec_capacity(vec: &Vec<usize>) -> usize {
    vec.capacity()
}

pub fn vec_reserve(vec: &mut Vec<usize>) {
    vec.reserve(32)
}

pub fn vec_shrink_to(vec: &mut Vec<usize>) {
    vec.shrink_to(32)
}

pub fn vec_into_boxed_slice(vec: Vec<usize>) -> Box<[usize], Global> {
    vec.into_boxed_slice()
}

pub fn vec_as_slice(vec: &Vec<usize>) -> &[usize] {
    vec.as_slice()
}

pub fn vec_as_mut_slice(vec: &mut Vec<usize>) -> &mut [usize] {
    vec.as_mut_slice()
}

pub fn vec_as_ptr(vec: &Vec<usize>) -> *const usize {
    vec.as_ptr()
}

pub fn vec_as_mut_ptr(vec: &mut Vec<usize>) -> *mut usize {
    vec.as_mut_ptr()
}

pub fn vec_insert(vec: &mut Vec<usize>, i: usize, el: usize) {
    vec.insert(i, el);
}

pub fn vec_remove(vec: &mut Vec<usize>, i: usize) -> usize {
    vec.remove(i)
}

pub fn vec_leak<'a>(vec: Vec<usize>) -> &'a mut [usize] {
    vec.leak()
}

pub fn vec_contains(haystack: &Vec<usize>, needle: &usize) -> bool {
    haystack.contains(needle)
}

pub fn vec_len(vec: &Vec<usize>) -> usize {
    vec.len()
}

pub unsafe fn vec_from_raw_parts(
    ptr: *mut usize,
    length: usize,
    capacity: usize,
) -> Vec<usize, Global> {
    Vec::from_raw_parts(ptr, length, capacity)
}
