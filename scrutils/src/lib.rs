#![feature(rustc_private, box_patterns, min_specialization)]

extern crate either;
extern crate polonius_engine;
extern crate rustc_abi;
extern crate rustc_borrowck;
extern crate rustc_const_eval;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_hash;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_infer;
extern crate rustc_interface;
extern crate rustc_macros;
extern crate rustc_middle;
extern crate rustc_mir_dataflow;
extern crate rustc_serialize;
extern crate rustc_span;
extern crate rustc_trait_selection;
extern crate rustc_type_ir;

mod analyzer;
mod body_cache;
mod collector;
mod common;
mod important;
mod precheck;
mod selector;

pub use analyzer::{run as run_analysis, PurityAnalysisResult};
pub use body_cache::{dump_mir_and_borrowck_facts, substituted_mir};
pub use collector::Collector;
pub use important::ImportantLocals;
pub use precheck::precheck;
pub use selector::{select_functions, select_pprs};
