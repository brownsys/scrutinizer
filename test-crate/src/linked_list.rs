use std::collections::LinkedList;

#[doc = "pure"]
pub fn contains_linked_list(haystack: &LinkedList<usize>, needle: &usize) -> bool {
    haystack.contains(needle)
}
