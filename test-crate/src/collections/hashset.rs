use std::collections::HashSet;

#[doc = "pure"]
pub fn contains_hashset(haystack: &HashSet<usize>, needle: &usize) -> bool {
    haystack.contains(needle)
}
