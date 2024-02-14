use rustc_middle::mir::{Place, PlaceElem};
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;
use rustc_utils::PlaceExt;
use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

#[derive(Clone, Debug, Eq)]
pub struct NormalizedPlace<'tcx> {
    place: Place<'tcx>,
}

impl<'tcx> PartialEq for NormalizedPlace<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        let result = self.place.local == other.place.local
            && self.place.projection.len() == other.place.projection.len()
            && self
                .place
                .projection
                .iter()
                .zip(other.place.projection.iter())
                .all(|(self_proj, other_proj)| match self_proj {
                    PlaceElem::Field(field_idx_self, ..) => match other_proj {
                        PlaceElem::Field(field_idx_other, ..) => field_idx_self == field_idx_other,
                        _ => false,
                    },
                    _ => self_proj == other_proj,
                });
        result
    }
}

impl<'tcx> Hash for NormalizedPlace<'tcx> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.place.local.hash(state);
        self.place.projection.iter().for_each(|proj| match proj {
            PlaceElem::Field(field_idx, ..) => field_idx.hash(state),
            _ => proj.hash(state),
        });
    }
}

impl<'tcx> NormalizedPlace<'tcx> {
    pub fn from_place(place: &Place<'tcx>, tcx: TyCtxt<'tcx>, def_id: DefId) -> Self {
        NormalizedPlace {
            place: place.normalize(tcx, def_id),
        }
    }
}

impl<'tcx> Deref for NormalizedPlace<'tcx> {
    type Target = Place<'tcx>;

    fn deref(&self) -> &Self::Target {
        &self.place
    }
}

impl<'tcx> Serialize for NormalizedPlace<'tcx> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(format!("{:?}", self.place).as_str())
    }
}
