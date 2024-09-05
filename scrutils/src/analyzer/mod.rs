mod analyzer;
mod deps;
mod heuristics;
mod result;

pub use analyzer::run;
pub use result::PurityAnalysisResult;
pub use deps::{compute_deps_for_body, compute_dep_strings_for_crates}; 