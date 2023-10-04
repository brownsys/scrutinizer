use std::collections::BTreeMap;

pub fn contains_btreemap(haystack: &BTreeMap<usize, usize>, needle: &usize) -> bool {
    haystack.contains_key(needle)
}
