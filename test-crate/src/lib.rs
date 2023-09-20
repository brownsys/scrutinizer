#![feature(const_trait_impl)]
#![feature(const_refs_to_cell)]

use std::marker::Destruct;

use std::collections::{BTreeMap, HashMap, LinkedList};
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

// Pure function: checking whether a linked list contains an item.
pub fn contains_linked_list(haystack: &LinkedList<usize>, needle: &usize) -> bool {
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

// Pure function: checking whether a binary tree map contains an item.
pub fn contains_btreemap(haystack: &BTreeMap<usize, usize>, needle: &usize) -> bool {
    haystack.contains_key(needle)
}

// Pure function: retrieving Vec's length.
pub fn len_vec(vec: &Vec<usize>) -> usize {
    vec.len()
}

// Raw mut pointer dereference.
pub unsafe fn raw_mut_ptr_deref() {
    let mut x = 42;
    let raw = &mut x as *mut i32;
    let _points_at = *raw;
}

// Raw mut pointer dereference outer function.
pub unsafe fn raw_mut_ptr_deref_outer() {
    raw_mut_ptr_deref();
}

// Raw const pointer dereference.
pub unsafe fn raw_const_ptr_deref() {
    let x = 42;
    let raw = &x as *const i32;
    let _points_at = *raw;
}

// Raw const pointer dereference outer function.
pub unsafe fn raw_const_ptr_deref_outer() {
    raw_const_ptr_deref();
}

pub fn lambda_called(a: usize) -> usize {
    let l = |x| {
        return x * x;
    };

    l(a)
}

pub fn lambda_uncalled(a: usize) -> usize {
    let _l = |x: usize| -> usize {
        return x * x;
    };
    a
}

// This works fine even with FnOnce or FnMut.
pub fn execute<F: FnOnce(usize) -> usize>(x: usize, l: F) -> usize {
    l(x)
}

// This example *might* drop types, but it doesn't happen in this case.
#[inline]
#[must_use]
pub const fn execute_destruct<T, F: ~ const FnOnce(&T) -> bool>(x: T, l: F) -> bool
    where
        T: ~ const Destruct,
        F: ~ const Destruct,
{
    l(&x)
}

// This is an example of dynamic dispatch, which does not let compiler determine the type of l.
pub fn execute_dyn(x: usize, l: &dyn Fn(usize) -> usize) -> usize {
    l(x)
}

pub fn closure_test(a: usize) {
    let lambda = |x: usize| -> usize {
        return x * x;
    };

    let lambda_ref = |x: &usize| -> bool {
        return *x > 0;
    };

    let y = 42;
    let closure_capture = |x: usize| -> usize  {
        return x * y;
    };

    let y = 42;
    let closure_capture_move = move |x: usize| -> usize  {
        return x * y;
    };

    let y = 42;
    let ambiguous_lambda = if y > 5 {
        |x: usize| -> usize  {
            return x;
        }
    } else {
        |x: usize| -> usize  {
            return x * x;
        }
    };

    // execute(a, lambda);
    // execute_destruct(a, lambda_ref);
    // execute_dyn(a, &lambda);
    // execute(a, closure_capture);
    // execute(a, closure_capture_move);
    // execute(a, ambiguous_lambda);
}