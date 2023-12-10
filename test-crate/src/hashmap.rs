use std::collections::hash_map::*;
use std::collections::HashMap;

pub fn hashmap_contains_key(hashmap: &HashMap<usize, usize>, element: &usize) -> bool {
    hashmap.contains_key(element)
}

pub fn hashmap_new() -> HashMap<usize, usize> {
    HashMap::new()
}

pub fn hashmap_with_capacity(capacity: usize) -> HashMap<usize, usize> {
    HashMap::with_capacity(capacity)
}

pub fn hashmap_keys(hashmap: &HashMap<usize, usize>) -> Keys<'_, usize, usize> {
    hashmap.keys()
}

pub fn hashmap_into_keys(hashmap: HashMap<usize, usize>) -> IntoKeys<usize, usize> {
    hashmap.into_keys()
}

pub fn hashmap_values(hashmap: &HashMap<usize, usize>) -> Values<'_, usize, usize> {
    hashmap.values()
}

pub fn hashmap_values_mut(hashmap: &mut HashMap<usize, usize>) -> ValuesMut<'_, usize, usize> {
    hashmap.values_mut()
}

pub fn hashmap_into_values(hashmap: HashMap<usize, usize>) -> IntoValues<usize, usize> {
    hashmap.into_values()
}

pub fn hashmap_iter(hashmap: &HashMap<usize, usize>) -> Iter<'_, usize, usize> {
    hashmap.iter()
}

pub fn hashmap_iter_mut(hashmap: &mut HashMap<usize, usize>) -> IterMut<'_, usize, usize> {
    hashmap.iter_mut()
}

pub fn hashmap_len(hashmap: &HashMap<usize, usize>) -> usize {
    hashmap.len()
}

pub fn hashmap_is_empty(hashmap: &HashMap<usize, usize>) -> bool {
    hashmap.is_empty()
}

pub fn hashmap_drain(hashmap: &mut HashMap<usize, usize>) -> Drain<'_, usize, usize> {
    hashmap.drain()
}

pub fn hashmap_retain<F>(hashmap: &mut HashMap<usize, usize>, f: F)
where
    F: FnMut(&usize, &mut usize) -> bool,
{
    hashmap.retain(f)
}

pub fn hashmap_clear(hashmap: &mut HashMap<usize, usize>) {
    hashmap.clear()
}

pub fn hashmap_hasher(hashmap: &HashMap<usize, usize, RandomState>) -> &RandomState {
    hashmap.hasher()
}
