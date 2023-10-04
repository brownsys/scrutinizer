use std::collections::HashMap;

pub fn contains_hashmap(haystack: &HashMap<usize, usize>, needle: &usize) -> bool {
    haystack.contains_key(needle)
}