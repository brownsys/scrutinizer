use std::collections::HashMap;
use std::collections::HashSet;

use uuid::Uuid;

// Calling a function from a foreign crate.
pub fn foreign_crate(left: usize, right: usize) -> usize {
    let _id = Uuid::new_v4();
    left + right
}

// Function with a side effect.
pub fn println_side_effect(left: usize, right: usize) -> usize {
    println!("{} {}", left, right);
    left + right
}

// Pure arithmetic function.
pub fn add(left: usize, right: usize) -> usize {
    left + right
}

// Function with pure body but mutable arguments.
pub fn add_mut(left: &mut usize, right: &mut usize) -> usize {
    *left + *right
}

// Function that calls a function that accepts arguments by mutable reference.
pub fn add_mut_wrapper(left: &mut usize, right: &mut usize) -> usize {
    add_mut(left, right)
}

// Pure function: checking whether a vector contains an item.
pub fn contains_vec(haystack: &Vec<usize>, needle: &usize) -> bool {
    haystack.contains(needle)
}

// Pure function: checking whether a hash map contains a key.
pub fn contains_hashmap(haystack: &HashMap<usize, usize>, needle: &usize) -> bool {
    haystack.contains_key(needle)
}

// Pure function: checking whether a hash set contains an item.
pub fn contains_hashset(haystack: &HashSet<usize>, needle: &usize) -> bool {
    haystack.contains(needle)
}

// Raw pointer dereference.
pub unsafe fn raw_ptr_deref() {
    let mut x = 42;
    let raw = &mut x as *mut i32;
    let _points_at = *raw;
}

// Raw pointer dereference outer function.
pub unsafe fn raw_ptr_deref_outer() {
    raw_ptr_deref();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}