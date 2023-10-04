use std::collections::HashSet;

pub fn contains_hashset(haystack: &HashSet<usize>, needle: &usize) -> bool {
    haystack.contains(needle)
}
