#![feature(box_patterns)]
#![feature(rustc_private)]
#![feature(min_specialization)]

extern crate rustc_abi;
extern crate rustc_borrowck;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_infer;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_mir_dataflow;
extern crate rustc_serialize;
extern crate rustc_span;
extern crate rustc_target;
extern crate rustc_trait_selection;

mod analyzer;
mod collector;
mod common;
mod important;
mod precheck;
mod selector;

pub use analyzer::{run as run_analysis, PurityAnalysisResult};
pub use collector::Collector;
pub use important::ImportantLocals;
pub use precheck::precheck;
pub use selector::{select_functions, select_pprs};
