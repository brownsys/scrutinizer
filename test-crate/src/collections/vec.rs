use crate::redefine;
use std::alloc::Global;
use std::cmp::Ordering;
use std::collections::TryReserveError;
use std::mem::MaybeUninit;
use std::ops::{Range, RangeBounds};
use std::slice::{
    Chunks, ChunksExact, ChunksExactMut, ChunksMut, EscapeAscii, Iter, IterMut, RChunks,
    RChunksExact, RChunksExactMut, RChunksMut, Windows,
};
use std::vec::Drain;

// Natively available methods.
redefine! { <Vec<usize>>::append => vec: &mut Vec<usize>, other: &mut Vec<usize, Global> => (), "impure" }
redefine! { <Vec<usize>>::as_mut_ptr => vec: &mut Vec<usize> => *mut usize, "impure" }
redefine! { <Vec<usize>>::as_mut_slice => vec: &mut Vec<usize> => &mut [usize], "impure" }
redefine! { <Vec<usize>>::as_ptr => vec: &Vec<usize> => *const usize, "pure" }
redefine! { <Vec<usize>>::as_slice => vec: &Vec<usize> => &[usize], "pure" }
redefine! { <Vec<usize>>::capacity => vec: &Vec<usize> => usize, "pure" }
redefine! { <Vec<usize>>::clear => vec: &mut Vec<usize> => (), "impure" }
redefine! { <Vec<usize>>::dedup => vec: &mut Vec<usize> => (), "impure" }
redefine! { <Vec<usize>>::dedup_by => vec: &mut Vec<usize>, same_bucket: impl FnMut(&mut usize, &mut usize) -> bool => (), "impure" }
redefine! { <Vec<usize>>::dedup_by_key => vec: &mut Vec<usize>, key: impl FnMut(&mut usize) -> usize => (), "impure" }
redefine! { <Vec<usize>>::drain => vec: &mut Vec<usize>, range: impl RangeBounds<usize> => Drain<'_, usize, Global>, "impure" }
redefine! { <Vec<usize>>::extend_from_slice => vec: &mut Vec<usize>, other: &[usize] => (), "impure" }
redefine! { <Vec<usize>>::extend_from_within => vec: &mut Vec<usize>, src: impl RangeBounds<usize> => (), "impure" }
redefine! { <Vec<usize>>::insert => vec: &mut Vec<usize>, index: usize, element: usize => (), "impure" }
redefine! { <Vec<usize>>::into_boxed_slice => vec: Vec<usize> => Box<[usize], Global>, "pure" }
redefine! { <Vec<usize>>::is_empty => vec: &Vec<usize> => bool, "pure" }
redefine! { <Vec<usize>>::leak => vec: Vec<usize> => &'static mut [usize], "pure" }
redefine! { <Vec<usize>>::len => vec: &Vec<usize> => usize, "pure" }
redefine! { <Vec<usize>>::new => => Vec<usize, Global>, "pure" }
redefine! { <Vec<usize>>::pop => vec: &mut Vec<usize> => Option<usize>, "impure" }
redefine! { <Vec<usize>>::push => vec: &mut Vec<usize>, value: usize => (), "impure" }
redefine! { <Vec<usize>>::remove => vec: &mut Vec<usize>, index: usize => usize, "impure" }
redefine! { <Vec<usize>>::reserve => vec: &mut Vec<usize>, additional: usize => (), "impure" }
redefine! { <Vec<usize>>::reserve_exact => vec: &mut Vec<usize>, additional: usize => (), "impure" }
redefine! { <Vec<usize>>::resize => vec: &mut Vec<usize>, new_len: usize, value: usize => (), "impure" }
redefine! { <Vec<usize>>::resize_with => vec: &mut Vec<usize>, new_len: usize, f: impl FnMut() -> usize => (), "impure" }
redefine! { <Vec<usize>>::retain => vec: &mut Vec<usize>, f: impl FnMut(&usize) -> bool => (), "impure" }
redefine! { <Vec<usize>>::retain_mut => vec: &mut Vec<usize>, f: impl FnMut(&mut usize) -> bool => (), "impure" }
redefine! { <Vec<usize>>::shrink_to => vec: &mut Vec<usize>, min_capacity: usize => (), "impure" }
redefine! { <Vec<usize>>::shrink_to_fit => vec: &mut Vec<usize> => (), "impure" }
redefine! { <Vec<usize>>::spare_capacity_mut => vec: &mut Vec<usize> => &mut [MaybeUninit<usize>], "impure" }
redefine! { <Vec<usize>>::split_off => vec: &mut Vec<usize>, at: usize => Vec<usize, Global>, "impure" }
redefine! { <Vec<usize>>::swap_remove => vec: &mut Vec<usize>, index: usize => usize, "impure" }
redefine! { <Vec<usize>>::truncate => vec: &mut Vec<usize>, len: usize => (), "impure" }
redefine! { <Vec<usize>>::try_reserve => vec: &mut Vec<usize>, additional: usize => Result<(), TryReserveError>, "impure" }
redefine! { <Vec<usize>>::try_reserve_exact => vec: &mut Vec<usize>, additional: usize => Result<(), TryReserveError>, "impure" }
redefine! { <Vec<usize>>::with_capacity => capacity: usize => Vec<usize, Global>, "pure" }

// Methods implicitly implemented by Deref<Target=[T]>.
redefine! { <[_]>::as_mut_ptr_range => vec: &mut Vec<usize> => Range<*mut usize>, "impure" }
redefine! { <[_]>::as_ptr_range => vec: &Vec<usize> => Range<*const usize>, "pure" }
redefine! { <[_]>::binary_search => vec: &Vec<usize>, x: &usize => Result<usize, usize>, "pure" }
redefine! { <[_]>::binary_search_by<'a> => vec: &'a Vec<usize>, f: impl FnMut(&'a usize) -> Ordering => Result<usize, usize>, "impure" }
redefine! { <[_]>::binary_search_by_key<'a> => vec: &'a Vec<usize>, b: &usize, f: impl FnMut(&'a usize) -> usize => Result<usize, usize>, "impure" }
redefine! { <[_]>::chunks => vec: &Vec<usize>, chunk_size: usize => Chunks<'_, usize>, "pure" }
redefine! { <[_]>::chunks_exact => vec: &Vec<usize>, chunk_size: usize => ChunksExact<'_, usize>, "pure" }
redefine! { <[_]>::chunks_exact_mut => vec: &mut Vec<usize>, chunk_size: usize => ChunksExactMut<'_, usize>, "impure" }
redefine! { <[_]>::chunks_mut => vec: &mut Vec<usize>, chunk_size: usize => ChunksMut<'_, usize>, "impure" }
redefine! { <[_]>::clone_from_slice => vec: &mut Vec<usize>, src: &[usize] => (), "impure" }
redefine! { <[_]>::contains => vec: &Vec<usize>, x: &usize => bool, "pure" }
redefine! { <[_]>::copy_from_slice => vec: &mut Vec<usize>, src: &[usize] => (), "impure" }
redefine! { <[_]>::copy_within => vec: &mut Vec<usize>, src: impl RangeBounds<usize>, dest: usize => (), "impure" }
redefine! { <[_]>::ends_with => vec: &Vec<usize>, needle: &[usize] => bool, "pure" }
redefine! { <[_]>::fill => vec: &mut Vec<usize>, value: usize => (), "impure" }
redefine! { <[_]>::fill_with => vec: &mut Vec<usize>, f: impl FnMut() -> usize => (), "impure" }
redefine! { <[_]>::first => vec: &Vec<usize> => Option<&usize>, "pure" }
redefine! { <[_]>::first_mut => vec: &mut Vec<usize> => Option<&mut usize>, "impure" }
redefine! { <[_]>::get => vec: &Vec<usize>, index: usize => Option<&usize>, "pure" }
redefine! { <[_]>::get_mut => vec: &mut Vec<usize>, index: usize => Option<&mut usize>, "impure" }
redefine! { <[_]>::iter => vec: &Vec<usize> => Iter<'_, usize>, "pure" }
redefine! { <[_]>::iter_mut => vec: &mut Vec<usize> => IterMut<'_, usize>, "impure" }
redefine! { <[_]>::last => vec: &Vec<usize> => Option<&usize>, "pure" }
redefine! { <[_]>::last_mut => vec: &mut Vec<usize> => Option<&mut usize>, "impure" }
redefine! { <[_]>::partition_point => vec: &Vec<usize>, pred: impl FnMut(&usize) -> bool => usize, "impure" }
redefine! { <[_]>::rchunks => vec: &Vec<usize>, chunk_size: usize => RChunks<'_, usize>, "pure" }
redefine! { <[_]>::rchunks_exact => vec: &Vec<usize>, chunk_size: usize => RChunksExact<'_, usize>, "pure" }
redefine! { <[_]>::rchunks_exact_mut => vec: &mut Vec<usize>, chunk_size: usize => RChunksExactMut<'_, usize>, "impure" }
redefine! { <[_]>::rchunks_mut => vec: &mut Vec<usize>, chunk_size: usize => RChunksMut<'_, usize>, "impure" }
redefine! { <[_]>::repeat => vec: &Vec<usize>, n: usize => Vec<usize, Global>, "pure" }
redefine! { <[_]>::reverse => vec: &mut Vec<usize> => (), "impure" }
redefine! { <[_]>::rotate_left => vec: &mut Vec<usize>, mid: usize => (), "impure" }
redefine! { <[_]>::rotate_right => vec: &mut Vec<usize>, k: usize => (), "impure" }
redefine! { <[_]>::select_nth_unstable => vec: &mut Vec<usize>, index: usize => (&mut [usize], &mut usize, &mut [usize]), "impure" }
redefine! { <[_]>::select_nth_unstable_by => vec: &mut Vec<usize>, index: usize, compare: impl FnMut(&usize, &usize) -> Ordering => (&mut [usize], &mut usize, &mut [usize]), "impure" }
redefine! { <[_]>::select_nth_unstable_by_key => vec: &mut Vec<usize>, index: usize, f: impl FnMut(&usize) -> usize => (&mut [usize], &mut usize, &mut [usize]), "impure" }
redefine! { <[_]>::sort => vec: &mut Vec<usize> => (), "impure" }
redefine! { <[_]>::sort_by => vec: &mut Vec<usize>, compare: impl FnMut(&usize, &usize) -> Ordering => (), "impure" }
redefine! { <[_]>::sort_by_cached_key => vec: &mut Vec<usize>, f: impl FnMut(&usize) -> usize => (), "impure" }
redefine! { <[_]>::sort_by_key => vec: &mut Vec<usize>, f: impl FnMut(&usize) -> usize => (), "impure" }
redefine! { <[_]>::sort_unstable => vec: &mut Vec<usize> => (), "impure" }
redefine! { <[_]>::sort_unstable_by => vec: &mut Vec<usize>, compare: impl FnMut(&usize, &usize) -> Ordering => (), "impure" }
redefine! { <[_]>::sort_unstable_by_key => vec: &mut Vec<usize>, f: impl FnMut(&usize) -> usize => (), "impure" }
redefine! { <[_]>::split_at => vec: &Vec<usize>, mid: usize => (&[usize], &[usize]), "pure" }
redefine! { <[_]>::split_at_mut => vec: &mut Vec<usize>, mid: usize => (&mut [usize], &mut [usize]), "impure" }
redefine! { <[_]>::split_first => vec: &Vec<usize> => Option<(&usize, &[usize])>, "pure" }
redefine! { <[_]>::split_first_mut => vec: &mut Vec<usize> => Option<(&mut usize, &mut [usize])>, "impure" }
redefine! { <[_]>::split_last => vec: &Vec<usize> => Option<(&usize, &[usize])>, "pure" }
redefine! { <[_]>::split_last_mut => vec: &mut Vec<usize> => Option<(&mut usize, &mut [usize])>, "impure" }
redefine! { <[_]>::starts_with => vec: &Vec<usize>, needle: &[usize] => bool, "pure" }
redefine! { <[_]>::swap => vec: &mut Vec<usize>, a: usize, b: usize => (), "impure" }
redefine! { <[_]>::swap_with_slice => vec: &mut Vec<usize>, other: &mut [usize] => (), "impure" }
redefine! { <[_]>::to_vec => vec: &Vec<usize> => Vec<usize, Global>, "pure" }
redefine! { <[_]>::windows => vec: &Vec<usize>, size: usize => Windows<'_, usize>, "pure" }

// ASCII-related methods.
redefine! { <[_]>::eq_ignore_ascii_case => vec: &Vec<u8>, other: &[u8] => bool, "pure" }
redefine! { <[_]>::escape_ascii => vec: &Vec<u8> => EscapeAscii<'_>, "pure" }
redefine! { <[_]>::is_ascii => vec: &Vec<u8> => bool, "pure" }
redefine! { <[_]>::make_ascii_lowercase => vec: &mut Vec<u8> => (), "impure" }
redefine! { <[_]>::make_ascii_uppercase => vec: &mut Vec<u8> => (), "impure" }
redefine! { <[_]>::to_ascii_lowercase => vec: &Vec<u8> => Vec<u8, Global>, "pure" }
redefine! { <[_]>::to_ascii_uppercase => vec: &Vec<u8> => Vec<u8, Global>, "pure" }
