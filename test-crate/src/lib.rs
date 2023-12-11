#![feature(allocator_api)]
#![feature(const_trait_impl)]
#![feature(const_refs_to_cell)]
#![allow(dead_code, unused_variables)]

mod btreemap;
mod hashmap;
mod hashset;
mod lambdas;
mod linked_list;
mod misc;
mod raw_ptr;
mod vartrack;
mod vec;

macro_rules! redefine {
    ($origin_struct:ident :: $func_ident:ident => $($param_ident:ident : $param_ty:ty),* => $ret_ty:ty) => {
        pub fn $func_ident($($param_ident : $param_ty),*) -> $ret_ty {
            $origin_struct::$func_ident($($param_ident),*)
        }
    };
    ($origin_struct:ident :: $func_ident:ident<$($lt:lifetime),+> => $($param_ident:ident : $param_ty:ty),* => $ret_ty:ty) => {
        pub fn $func_ident<$($lt),*>($($param_ident : $param_ty),*) -> $ret_ty {
            $origin_struct::$func_ident($($param_ident),*)
        }
    };
    ($new_ident:ident, $origin_struct:ident :: $func_ident:ident => $($param_ident:ident : $param_ty:ty),* => $ret_ty:ty) => {
        pub fn $new_ident($($param_ident : $param_ty),*) -> $ret_ty {
            $origin_struct::$func_ident($($param_ident),*)
        }
    };
    ($new_ident:ident, $origin_struct:ident :: $func_ident:ident<$($lt:lifetime),+> => $($param_ident:ident : $param_ty:ty),* => $ret_ty:ty) => {
        pub fn $new_ident<$($lt),*>($($param_ident : $param_ty),*) -> $ret_ty {
            $origin_struct::$func_ident($($param_ident),*)
        }
    };
}

pub(crate) use redefine;
