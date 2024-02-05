use rustc_middle::mir::Place;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;
use rustc_utils::PlaceExt;
use serde::Serialize;
use std::ops::Deref;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NormalizedPlace<'tcx> {
    place: Place<'tcx>,
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
