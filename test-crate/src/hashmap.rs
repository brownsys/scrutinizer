use crate::redefine;
use std::collections::hash_map::*;
use std::collections::HashMap;
use std::collections::TryReserveError;
use std::fmt::Debug;
use std::fmt::{Formatter, Result as FmtResult};
use std::ops::Index;

redefine! { <HashMap<usize, usize>>::capacity => hashmap: &HashMap<usize, usize> => usize }
redefine! { <HashMap<usize, usize>>::clone => hashmap: &HashMap<usize, usize> => HashMap<usize, usize> }
redefine! { <HashMap<usize, usize>>::clone_from => hashmap: &mut HashMap<usize, usize>, other: &HashMap<usize, usize> => () }
redefine! { <HashMap<usize, usize>>::clear => hashmap: &mut HashMap<usize, usize> => () }
redefine! { <HashMap<usize, usize>>::contains_key => hashmap: &HashMap<usize, usize>, element: &usize => bool }
redefine! { <HashMap<usize, usize>>::drain => hashmap: &mut HashMap<usize, usize> => Drain<'_, usize, usize> }
redefine! { <HashMap<usize, usize>>::default => => HashMap<usize, usize> }
redefine! { <HashMap<usize, usize>>::entry => hashmap: &mut HashMap<usize, usize>, key: usize => Entry<'_, usize, usize> }
redefine! { extend_1, <HashMap<usize, usize>>::extend<'a> => hashmap: &mut HashMap<usize, usize>, iter: impl IntoIterator<Item = (&'a usize, &'a usize)> => () }
redefine! { extend_2, <HashMap<usize, usize>>::extend => hashmap: &mut HashMap<usize, usize>, iter: impl IntoIterator<Item = (usize, usize)> => () }
redefine! { <HashMap<usize, usize>>::eq => hashmap: &HashMap<usize, usize>, other: &HashMap<usize, usize> => bool }
redefine! { <HashMap<usize, usize>>::from => arr: [(usize, usize); 10] => HashMap<usize, usize> } // rejected
redefine! { <HashMap<usize, usize>>::from_iter => iter: impl IntoIterator<Item = (usize, usize)> => HashMap<usize, usize> }
redefine! { <HashMap<usize, usize>>::fmt => hashmap: &HashMap<usize, usize>, f: &mut Formatter<'_> => FmtResult }
redefine! { <HashMap<usize, usize>>::get<'a> => hashmap: &'a HashMap<usize, usize>, k: &usize => Option<&'a usize> }
redefine! { <HashMap<usize, usize>>::get_key_value<'a, 'b> => hashmap: &'a HashMap<usize, usize>, k: &'a usize => Option<(&'a usize, &'a usize)>  }
redefine! { <HashMap<usize, usize>>::get_mut<'a> => hashmap: &'a mut HashMap<usize, usize>, k: &usize => Option<&'a mut usize> }
redefine! { <HashMap<usize, usize>>::hasher => hashmap: &HashMap<usize, usize> => &RandomState }
redefine! { <HashMap<usize, usize>>::index<'a> => hashmap: &'a HashMap<usize, usize>, key: &'a usize => &'a usize }
redefine! { <HashMap<usize, usize>>::insert => hashmap: &mut HashMap<usize, usize>, k: usize, v: usize => Option<usize> }
redefine! { into_iter_1, <HashMap<usize, usize>>::into_iter => hashmap: HashMap<usize, usize> => IntoIter<usize, usize> }
pub fn into_iter_2(hashmap: &mut HashMap<usize, usize>) -> IterMut<'_, usize, usize> {
    hashmap.into_iter()
}

pub fn into_iter_3(hashmap: &HashMap<usize, usize>) -> Iter<'_, usize, usize> {
    hashmap.into_iter()
}
redefine! { <HashMap<usize, usize>>::into_keys => hashmap: HashMap<usize, usize> => IntoKeys<usize, usize> }
redefine! { <HashMap<usize, usize>>::into_values => hashmap: HashMap<usize, usize> => IntoValues<usize, usize> }
redefine! { <HashMap<usize, usize>>::is_empty => hashmap: &HashMap<usize, usize> => bool }
redefine! { <HashMap<usize, usize>>::iter => hashmap: &HashMap<usize, usize> => Iter<'_, usize, usize> }
redefine! { <HashMap<usize, usize>>::iter_mut => hashmap: &mut HashMap<usize, usize> => IterMut<'_, usize, usize> }
redefine! { <HashMap<usize, usize>>::keys => hashmap: &HashMap<usize, usize> => Keys<'_, usize, usize> }
redefine! { <HashMap<usize, usize>>::len => hashmap: &HashMap<usize, usize> => usize }
redefine! { <HashMap<usize, usize>>::ne => hashmap: &HashMap<usize, usize>, other: &HashMap<usize, usize> => bool }
redefine! { <HashMap<usize, usize>>::new => => HashMap<usize, usize, RandomState> }
redefine! { <HashMap<usize, usize>>::remove => hashmap: &mut HashMap<usize, usize>, k: &usize => Option<usize> }
redefine! { <HashMap<usize, usize>>::remove_entry => hashmap: &mut HashMap<usize, usize>, k: &usize => Option<(usize, usize)> }
redefine! { <HashMap<usize, usize>>::retain => hashmap: &mut HashMap<usize, usize>, f: impl FnMut(&usize, &mut usize) -> bool => () }
redefine! { <HashMap<usize, usize>>::reserve => hashmap: &mut HashMap<usize, usize>, additional: usize => () }
redefine! { <HashMap<usize, usize>>::shrink_to => hashmap: &mut HashMap<usize, usize>, min_capacity: usize => () }
redefine! { <HashMap<usize, usize>>::shrink_to_fit => hashmap: &mut HashMap<usize, usize> => () }
redefine! { <HashMap<usize, usize>>::try_reserve => hashmap: &mut HashMap<usize, usize>, additional: usize => Result<(), TryReserveError> }
redefine! { <HashMap<usize, usize>>::values => hashmap: &HashMap<usize, usize> => Values<'_, usize, usize> }
redefine! { <HashMap<usize, usize>>::values_mut => hashmap: &mut HashMap<usize, usize> => ValuesMut<'_, usize, usize> }
redefine! { <HashMap<usize, usize>>::with_capacity => capacity: usize => HashMap<usize, usize, RandomState> }
redefine! { <HashMap<usize, usize>>::with_capacity_and_hasher => capacity: usize, hasher: RandomState => HashMap<usize, usize, RandomState> }
redefine! { <HashMap<usize, usize>>::with_hasher => hash_builder: RandomState => HashMap<usize, usize, RandomState> }
