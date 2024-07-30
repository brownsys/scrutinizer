//! Alias analysis to determine the points-to set of a reference.

use std::{hash::Hash, ops::ControlFlow, rc::Rc, time::Instant};

use log::{debug, info};
use rustc_data_structures::{
    fx::{FxHashMap as HashMap, FxHashSet as HashSet},
    graph::{iterate::reverse_post_order, scc::Sccs, vec_graph::VecGraph},
    intern::Interned,
};
use rustc_hir::def_id::DefId;
use rustc_index::{
    bit_set::{HybridBitSet, SparseBitMatrix},
    vec::IndexVec,
};
use rustc_middle::{
    mir::{
        visit::{PlaceContext, Visitor},
        *,
    },
    ty::{
        Region, RegionKind, RegionVid, Ty, TyCtxt, TyKind, TypeSuperVisitable,
        TypeVisitor,
    },
};
use rustc_target::abi::FieldIdx;
use rustc_utils::{
    block_timer,
    cache::{Cache, CopyCache},
    timer::elapsed,
    MutabilityExt, PlaceExt,
};

use flowistry::{
    extensions::{is_extension_active, MutabilityMode, PointerMode},
    indexed::impls::{build_location_arg_domain, LocationOrArgDomain, PlaceSet},
    mir::utils::{self, AsyncHack},
};

use super::nll_facts::FlowistryFacts;

#[derive(Default)]
struct GatherBorrows<'tcx> {
    borrows: Vec<(RegionVid, BorrowKind, Place<'tcx>)>,
}

macro_rules! region_pat {
    ($name:ident) => {
        Region(Interned(RegionKind::ReVar($name), _))
    };
}

impl<'tcx> Visitor<'tcx> for GatherBorrows<'tcx> {
    fn visit_assign(&mut self, _place: &Place<'tcx>, rvalue: &Rvalue<'tcx>, _location: Location) {
        if let Rvalue::Ref(region_pat!(region), kind, borrowed_place) = rvalue {
            self.borrows.push((*region, *kind, *borrowed_place));
        }
    }
}

struct FindPlaces<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
    def_id: DefId,
    places: Vec<Place<'tcx>>,
}

impl<'tcx> Visitor<'tcx> for FindPlaces<'_, 'tcx> {
    // this is needed for eval? not sure why locals wouldn't show up in the body as places,
    // maybe optimized out or something
    fn visit_local_decl(&mut self, local: Local, _local_decl: &LocalDecl<'tcx>) {
        self.places.push(Place::from_local(local, self.tcx));
    }

    fn visit_place(&mut self, place: &Place<'tcx>, _context: PlaceContext, _location: Location) {
        self.places.push(*place);
    }

    fn visit_assign(&mut self, place: &Place<'tcx>, rvalue: &Rvalue<'tcx>, location: Location) {
        self.super_assign(place, rvalue, location);

        let is_borrow = matches!(rvalue, Rvalue::Ref(..));
        if is_borrow {
            self.places.push(self.tcx.mk_place_deref(*place));
        }

        // See PlaceCollector for where this matters
        if let Rvalue::Aggregate(box AggregateKind::Adt(def_id, idx, substs, _, _), _) = rvalue {
            let adt_def = self.tcx.adt_def(*def_id);
            let variant = adt_def.variant(*idx);
            let places = variant.fields.iter().enumerate().map(|(i, field)| {
                let mut projection = place.projection.to_vec();
                projection.push(ProjectionElem::Field(
                    FieldIdx::from_usize(i),
                    field.ty(self.tcx, substs),
                ));
                Place::make(place.local, &projection, self.tcx)
            });
            self.places.extend(places);
        }
    }

    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, location: Location) {
        self.super_terminator(terminator, location);

        match &terminator.kind {
            TerminatorKind::Call { args, .. } => {
                let arg_places = utils::arg_places(args);
                let arg_mut_ptrs =
                    utils::arg_mut_ptrs(&arg_places, self.tcx, self.body, self.def_id);
                self.places
                    .extend(arg_mut_ptrs.into_iter().map(|(_, place)| place));
            }

            _ => {}
        }
    }
}

type LoanSet<'tcx> = HashSet<(Place<'tcx>, Mutability)>;
type LoanMap<'tcx> = HashMap<RegionVid, LoanSet<'tcx>>;

/// Used to describe aliases of owned and raw pointers.
pub const UNKNOWN_REGION: RegionVid = RegionVid::MAX;

#[allow(dead_code)]
/// Data structure for computing and storing aliases.
pub struct Aliases<'a, 'tcx> {
    // Compiler data
    pub tcx: TyCtxt<'tcx>,
    pub body: &'a Body<'tcx>,
    pub def_id: DefId,
    location_domain: Rc<LocationOrArgDomain>,

    // Core computed data structure
    loans: LoanMap<'tcx>,

    // Caching for derived analysis
    normalized_cache: CopyCache<Place<'tcx>, Place<'tcx>>,
    aliases_cache: Cache<Place<'tcx>, PlaceSet<'tcx>>,
    conflicts_cache: Cache<Place<'tcx>, PlaceSet<'tcx>>,
    reachable_cache: Cache<(Place<'tcx>, Mutability), PlaceSet<'tcx>>,
}

rustc_index::newtype_index! {
  #[debug_format = "rs{}"]
  struct RegionSccIndex {}
}


impl<'a, 'tcx> Aliases<'a, 'tcx> {
    fn compute_loans(
        tcx: TyCtxt<'tcx>,
        def_id: DefId,
        body: &'a Body<'tcx>,
        facts: &'a FlowistryFacts,
    ) -> LoanMap<'tcx> {
        let start = Instant::now();
        let static_region = RegionVid::from_usize(0);
        let subset_base = &facts.subset_base;

        let all_pointers = body
            .local_decls()
            .indices()
            .flat_map(|local| Place::from_local(local, tcx).interior_pointers(tcx, body, def_id))
            .collect::<Vec<_>>();
        let max_region = all_pointers
            .iter()
            .map(|(region, _)| *region)
            .chain(subset_base.iter().flat_map(|(r1, r2, _)| [*r1, *r2]))
            .filter(|r| *r != UNKNOWN_REGION)
            .max()
            .unwrap_or(static_region);
        let num_regions = max_region.as_usize() + 1;
        let all_regions = (0..num_regions).map(RegionVid::from_usize);

        let mut subset = SparseBitMatrix::new(num_regions);

        let async_hack = AsyncHack::new(tcx, body, def_id);
        let ignore_regions = async_hack.ignore_regions();

        // subset('a, 'b) :- subset_base('a, 'b, _).
        for (a, b, _) in subset_base {
            if ignore_regions.contains(a) || ignore_regions.contains(b) {
                continue;
            }
            subset.insert(*a, *b);
        }

        // subset('static, 'a).
        for a in all_regions.clone() {
            subset.insert(static_region, a);
        }

        if is_extension_active(|mode| mode.pointer_mode == PointerMode::Conservative) {
            // for all p1 : &'a T, p2: &'b T: subset('a, 'b).
            let mut region_to_pointers: HashMap<_, Vec<_>> = HashMap::default();
            for (region, places) in &all_pointers {
                if *region != UNKNOWN_REGION {
                    region_to_pointers
                        .entry(*region)
                        .or_default()
                        .extend(places);
                }
            }

            let constraints = generate_conservative_constraints(tcx, &body, &region_to_pointers);

            for (a, b) in constraints {
                subset.insert(a, b);
            }
        }

        let mut contains: LoanMap<'tcx> = HashMap::default();
        let mut definite: HashMap<RegionVid, (Ty<'tcx>, Vec<PlaceElem<'tcx>>)> = HashMap::default();

        // For all e = &'a x.q in body:
        //   contains('a, p).
        //   If p = p^[* p2]: definite('a, ty(p2), p2^[])
        //   Else:            definite('a, ty(p),  p^[]).
        let mut gather_borrows = GatherBorrows::default();
        gather_borrows.visit_body(&body);
        for (region, kind, place) in gather_borrows.borrows {
            if place.is_direct(body) {
                contains
                    .entry(region)
                    .or_default()
                    .insert((place, kind.to_mutbl_lossy()));
            }

            let def = match place.refs_in_projection().next() {
                Some((ptr, proj)) => {
                    let ptr_ty = ptr.ty(body.local_decls(), tcx).ty;
                    (ptr_ty.builtin_deref(true).unwrap().ty, proj.to_vec())
                }
                None => (
                    body.local_decls()[place.local].ty,
                    place.projection.to_vec(),
                ),
            };
            definite.insert(region, def);
        }

        // For all args p : &'a ω T where 'a is abstract: contains('a, *p, ω).
        for arg in body.args_iter() {
            for (region, places) in Place::from_local(arg, tcx).interior_pointers(tcx, body, def_id)
            {
                let region_contains = contains.entry(region).or_default();
                for (place, mutability) in places {
                    // WARNING / TODO: this is a huge hack (that is conjoined w/ all_args).
                    // Need a way to limit the number of possible pointers for functions with
                    // many pointers in the input. This is almost certainly not sound, but hopefully
                    // it works for most cases.
                    if place.projection.len() <= 2 {
                        region_contains.insert((tcx.mk_place_deref(place), mutability));
                    }
                }
            }
        }

        // For all places p : *T or p : Box<T>: contains('UNK, *p, mut).
        let unk_contains = contains.entry(UNKNOWN_REGION).or_default();
        for (region, places) in &all_pointers {
            if *region == UNKNOWN_REGION {
                for (place, _) in places {
                    unk_contains.insert((tcx.mk_place_deref(*place), Mutability::Mut));
                }
            }
        }

        info!(
            "Initial places in loan set: {}, total regions {}, definite regions: {}",
            contains.values().map(|set| set.len()).sum::<usize>(),
            contains.len(),
            definite.len()
        );

        debug!("Initial contains: {contains:#?}");
        debug!("Definite: {definite:#?}");

        // Compute a topological sort of the subset relation.
        let edge_pairs = subset
            .rows()
            .flat_map(|r1| subset.iter(r1).map(move |r2| (r1, r2)))
            .collect::<Vec<_>>();
        let subset_graph = VecGraph::new(num_regions, edge_pairs);
        let subset_sccs = Sccs::<RegionVid, RegionSccIndex>::new(&subset_graph);
        let mut scc_to_regions =
            IndexVec::from_elem_n(HybridBitSet::new_empty(num_regions), subset_sccs.num_sccs());
        for r in all_regions.clone() {
            let scc = subset_sccs.scc(r);
            scc_to_regions[scc].insert(r);
        }
        let scc_order = reverse_post_order(&subset_sccs, subset_sccs.scc(static_region));
        elapsed("relation construction", start);

        // Subset implies containment: l ∈ 'a ∧ 'a ⊆ 'b ⇒ l ∈ 'b
        // i.e. contains('b, l) :- contains('a, l), subset('a, 'b).
        //
        // contains('b, p2^[p], ω) :-
        //   contains('a, p, ω), subset('a, 'b),
        //   definite('b, T, p2^[]), !subset('b, 'a), p : T.
        //
        // If 'a is from a borrow expression &'a proj[*p'], then we add proj to all inherited aliases.
        // See interprocedural_field_independence for an example where this matters.
        // But we only do this if:
        //   * !subset('b, 'a) since otherwise projections would be added infinitely.
        //   * if p' : &T, then p : T since otherwise proj[p] is not well-typed.
        //
        // Note that it's theoretically more efficient to compute the transitive closure of `subset`
        // and then do the pass below in one step rather than to a fixpoint. But this negates the added
        // precision from propagating projections. For example, in the program:
        //   a = &'0 mut (0, 0)
        //   b = &'1 mut a.0
        //   c = &'2 mut *b
        //   *c = 1;
        // then '0 :> '1 :> '2. By propagating projections, then '1 = {a.0}. However if we see '0 :> '2
        // to insert contains('0) into contains('2), then contains('2) = {a, a.0} which defeats the purpose!
        // Then *c = 1 is considered to be a mutation to anything within a.
        //
        // Rather than iterating over the entire subset relation, we only do local fixpoints
        // within each strongly-connected component.
        let start = Instant::now();
        for r in all_regions {
            contains.entry(r).or_default();
        }
        for scc_idx in scc_order {
            loop {
                let mut changed = false;
                let scc = &scc_to_regions[scc_idx];
                for a in scc.iter() {
                    for b in subset.iter(a) {
                        if a == b {
                            continue;
                        }

                        // SAFETY: a != b
                        let a_contains =
                            unsafe { &*(contains.get(&a).unwrap() as *const LoanSet<'tcx>) };
                        let b_contains =
                            unsafe { &mut *(contains.get_mut(&b).unwrap() as *mut LoanSet<'tcx>) };

                        let cyclic = scc.contains(b);
                        match definite.get(&b) {
                            Some((ty, proj)) if !cyclic => {
                                for (p, mutability) in a_contains.iter() {
                                    let p_ty = p.ty(body.local_decls(), tcx).ty;
                                    let p_proj = if *ty == p_ty {
                                        let mut full_proj = p.projection.to_vec();
                                        full_proj.extend(proj);
                                        Place::make(p.local, tcx.mk_place_elems(&full_proj), tcx)
                                    } else {
                                        *p
                                    };

                                    changed |= b_contains.insert((p_proj, *mutability));
                                }
                            }
                            _ => {
                                let orig_len = b_contains.len();
                                b_contains.extend(a_contains);
                                changed |= b_contains.len() != orig_len;
                            }
                        }
                    }
                }

                if !changed {
                    break;
                }
            }
        }
        elapsed("fixpoint", start);

        info!(
            "Final places in loan set: {}",
            contains.values().map(|set| set.len()).sum::<usize>()
        );
        contains
    }

    pub fn build(
        tcx: TyCtxt<'tcx>,
        def_id: DefId,
        body: &'a Body<'tcx>,
        facts: &'a FlowistryFacts,
    ) -> Self {
        block_timer!("aliases");
        let location_domain = build_location_arg_domain(body);

        let loans = Self::compute_loans(tcx, def_id, body, facts);
        debug!("Loans: {loans:?}");

        Aliases {
            loans,
            tcx,
            body,
            def_id,
            location_domain,
            aliases_cache: Cache::default(),
            normalized_cache: CopyCache::default(),
            conflicts_cache: Cache::default(),
            reachable_cache: Cache::default(),
        }
    }

    pub fn location_domain(&self) -> &Rc<LocationOrArgDomain> {
        &self.location_domain
    }
}

fn generate_conservative_constraints<'tcx>(
    tcx: TyCtxt<'tcx>,
    body: &Body<'tcx>,
    region_to_pointers: &HashMap<RegionVid, Vec<(Place<'tcx>, Mutability)>>,
) -> Vec<(RegionVid, RegionVid)> {
    let get_ty = |p| tcx.mk_place_deref(p).ty(body.local_decls(), tcx).ty;
    let same_ty = |p1, p2| get_ty(p1) == get_ty(p2);

    region_to_pointers
        .iter()
        .flat_map(|(region, places)| {
            let regions_with_place = region_to_pointers
                .iter()
                // find other regions that contain a loan matching any type in places
                .filter(|(other_region, other_places)| {
                    *region != **other_region
                        && places.iter().any(|(place, _)| {
                            other_places
                                .iter()
                                .any(|(other_place, _)| same_ty(*place, *other_place))
                        })
                });

            // add 'a : 'b and 'b : 'a to ensure the lifetimes are considered equal
            regions_with_place
                .flat_map(|(other_region, _)| [(*region, *other_region), (*other_region, *region)])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

// TODO: this visitor shares some structure with the PlaceCollector in mir utils.
// Can we consolidate these?
struct LoanCollector<'a, 'tcx> {
    aliases: &'a Aliases<'a, 'tcx>,
    unknown_region: Region<'tcx>,
    target_mutability: Mutability,
    stack: Vec<Mutability>,
    loans: PlaceSet<'tcx>,
}

impl<'tcx> TypeVisitor<TyCtxt<'tcx>> for LoanCollector<'_, 'tcx> {
    type BreakTy = ();

    fn visit_ty(&mut self, ty: Ty<'tcx>) -> ControlFlow<Self::BreakTy> {
        match ty.kind() {
            TyKind::Ref(_, _, mutability) => {
                self.stack.push(*mutability);
                ty.super_visit_with(self);
                self.stack.pop();
                return ControlFlow::Break(());
            }
            _ if ty.is_box() || ty.is_unsafe_ptr() => {
                self.visit_region(self.unknown_region);
            }
            _ => {}
        };

        ty.super_visit_with(self);
        ControlFlow::Continue(())
    }

    fn visit_region(&mut self, region: Region<'tcx>) -> ControlFlow<Self::BreakTy> {
        let region = match region.kind() {
            RegionKind::ReVar(region) => region,
            RegionKind::ReStatic => RegionVid::from_usize(0),
            // TODO: do we need to handle bound regions?
            // e.g. shows up with closures, for<'a> ...
            RegionKind::ReErased | RegionKind::ReLateBound(_, _) => {
                return ControlFlow::Continue(());
            }
            _ => unreachable!("{region:?}"),
        };
        if let Some(loans) = self.aliases.loans.get(&region) {
            let under_immut_ref = self.stack.iter().any(|m| *m == Mutability::Not);
            let ignore_mut =
                is_extension_active(|mode| mode.mutability_mode == MutabilityMode::IgnoreMut);
            self.loans
                .extend(loans.iter().filter_map(|(place, mutability)| {
                    if ignore_mut {
                        return Some(place);
                    }
                    let loan_mutability = if under_immut_ref {
                        Mutability::Not
                    } else {
                        *mutability
                    };
                    self.target_mutability
                        .is_permissive_as(loan_mutability)
                        .then_some(place)
                }))
        }

        ControlFlow::Continue(())
    }
}
