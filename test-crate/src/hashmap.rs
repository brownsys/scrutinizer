use crate::redefine;
use std::collections::hash_map::*;
use std::collections::HashMap;
use std::collections::TryReserveError;
use std::fmt::Debug;
use std::fmt::{Formatter, Result as FmtResult};
use std::ops::Index;

redefine! { HashMap::capacity => hashmap: &HashMap<usize, usize> => usize }
redefine! { HashMap::clone => hashmap: &HashMap<usize, usize> => HashMap<usize, usize> }
redefine! { HashMap::clone_from => hashmap: &mut HashMap<usize, usize>, other: &HashMap<usize, usize> => () }
redefine! { HashMap::clear => hashmap: &mut HashMap<usize, usize> => () }
redefine! { HashMap::contains_key => hashmap: &HashMap<usize, usize>, element: &usize => bool }
redefine! { HashMap::drain => hashmap: &mut HashMap<usize, usize> => Drain<'_, usize, usize> }
redefine! { HashMap::default => => HashMap<usize, usize> }
redefine! { HashMap::entry => hashmap: &mut HashMap<usize, usize>, key: usize => Entry<'_, usize, usize> }
redefine! { extend_1, HashMap::extend<'a> => hashmap: &mut HashMap<usize, usize>, iter: impl IntoIterator<Item = (&'a usize, &'a usize)> => () }
redefine! { extend_2, HashMap::extend => hashmap: &mut HashMap<usize, usize>, iter: impl IntoIterator<Item = (usize, usize)> => () }
redefine! { HashMap::eq => hashmap: &HashMap<usize, usize>, other: &HashMap<usize, usize> => bool }
redefine! { HashMap::from => arr: [(usize, usize); 10] => HashMap<usize, usize> }
redefine! { HashMap::from_iter => iter: impl IntoIterator<Item = (usize, usize)> => HashMap<usize, usize> }
redefine! { HashMap::fmt => hashmap: &HashMap<usize, usize>, f: &mut Formatter<'_> => FmtResult }
redefine! { HashMap::get<'a> => hashmap: &'a HashMap<usize, usize>, k: &usize => Option<&'a usize> }
redefine! { HashMap::get_key_value<'a, 'b> => hashmap: &'a HashMap<usize, usize>, k: &'a usize => Option<(&'a usize, &'a usize)>  }
redefine! { HashMap::get_mut<'a> => hashmap: &'a mut HashMap<usize, usize>, k: &usize => Option<&'a mut usize> }
redefine! { HashMap::hasher => hashmap: &HashMap<usize, usize> => &RandomState }
redefine! { HashMap::index<'a> => hashmap: &'a HashMap<usize, usize>, key: &'a usize => &'a usize }
redefine! { HashMap::insert => hashmap: &mut HashMap<usize, usize>, k: usize, v: usize => Option<usize> }
redefine! { into_iter_1, HashMap::into_iter => hashmap: HashMap<usize, usize> => IntoIter<usize, usize> }
pub fn into_iter_2(hashmap: &mut HashMap<usize, usize>) -> IterMut<'_, usize, usize> {
    hashmap.into_iter()
}

pub fn into_iter_3(hashmap: &HashMap<usize, usize>) -> Iter<'_, usize, usize> {
    hashmap.into_iter()
}
redefine! { HashMap::into_keys => hashmap: HashMap<usize, usize> => IntoKeys<usize, usize> }
redefine! { HashMap::into_values => hashmap: HashMap<usize, usize> => IntoValues<usize, usize> }
redefine! { HashMap::is_empty => hashmap: &HashMap<usize, usize> => bool }
redefine! { HashMap::iter => hashmap: &HashMap<usize, usize> => Iter<'_, usize, usize> }
redefine! { HashMap::iter_mut => hashmap: &mut HashMap<usize, usize> => IterMut<'_, usize, usize> }
redefine! { HashMap::keys => hashmap: &HashMap<usize, usize> => Keys<'_, usize, usize> }
redefine! { HashMap::len => hashmap: &HashMap<usize, usize> => usize }
redefine! { HashMap::ne => hashmap: &HashMap<usize, usize>, other: &HashMap<usize, usize> => bool }
redefine! { HashMap::new => => HashMap<usize, usize, RandomState> }
redefine! { HashMap::remove => hashmap: &mut HashMap<usize, usize>, k: &usize => Option<usize> }
redefine! { HashMap::remove_entry => hashmap: &mut HashMap<usize, usize>, k: &usize => Option<(usize, usize)> }
redefine! { HashMap::retain => hashmap: &mut HashMap<usize, usize>, f: impl FnMut(&usize, &mut usize) -> bool => () }
redefine! { HashMap::reserve => hashmap: &mut HashMap<usize, usize>, additional: usize => () }
redefine! { HashMap::shrink_to => hashmap: &mut HashMap<usize, usize>, min_capacity: usize => () }
redefine! { HashMap::shrink_to_fit => hashmap: &mut HashMap<usize, usize> => () }
redefine! { HashMap::try_reserve => hashmap: &mut HashMap<usize, usize>, additional: usize => Result<(), TryReserveError> }
redefine! { HashMap::values => hashmap: &HashMap<usize, usize> => Values<'_, usize, usize> }
redefine! { HashMap::values_mut => hashmap: &mut HashMap<usize, usize> => ValuesMut<'_, usize, usize> }
redefine! { HashMap::with_capacity => capacity: usize => HashMap<usize, usize, RandomState> }
redefine! { HashMap::with_capacity_and_hasher => capacity: usize, hasher: RandomState => HashMap<usize, usize, RandomState> }
redefine! { HashMap::with_hasher => hash_builder: RandomState => HashMap<usize, usize, RandomState> }
