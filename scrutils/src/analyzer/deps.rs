use rustc_middle::mir::{visit::Visitor, Body, Location, Terminator, TerminatorKind};
use rustc_middle::ty::TyCtxt;
use std::collections::HashMap;
use std::collections::HashSet;
use std::process::Command;

struct CallCrateCollector<'tcx> {
    crates_from_calls: HashSet<String>,
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> Visitor<'tcx> for CallCrateCollector<'tcx> {
    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, _location: Location) {
        if let TerminatorKind::Call { func, .. } = &terminator.kind {
            if let Some((callee_def_id, _)) = func.const_fn_def() {
                self.crates_from_calls
                    .insert(self.tcx.crate_name(callee_def_id.krate).to_string());
            }
        }
    }
}

pub fn compute_deps_for_body<'tcx>(body: Body<'tcx>, tcx: TyCtxt<'tcx>) -> HashSet<String> {
    let mut collector = CallCrateCollector {
        crates_from_calls: HashSet::new(),
        tcx,
    };
    collector.visit_body(&body);
    collector.crates_from_calls
}

fn get_direct_deps() -> Vec<String> {
    let output = Command::new("cargo")
        .args(["tree", "--quiet", "--frozen", "--prefix=none", "--depth", "1"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    stdout
        .split("\n")
        .filter(|line| !line.is_empty())
        .map(|direct_dep| {
            let dep_and_ver: Vec<_> = direct_dep.split_whitespace().take(2).collect();
            format!("{}@{}", dep_and_ver[0], dep_and_ver[1].replace("v", ""))
        })
        .collect()
}

pub fn compute_dep_strings_for_crates(crate_names: &HashSet<String>) -> HashMap<String, String> {
    let direct_deps = get_direct_deps();
    crate_names.iter().filter_map(|crate_name| {
        direct_deps.iter().find(|dep_spec| dep_spec.contains(crate_name)).map(|crate_spec| {
            let output = Command::new("cargo")
            .args(["tree", "--quiet", "--frozen", "--no-dedupe", "--prefix=none", "--package", crate_spec])
            .output()
            .unwrap();
            let stdout = String::from_utf8(output.stdout).unwrap();
            (crate_name.clone(), stdout)
        })
    }).collect()
}
