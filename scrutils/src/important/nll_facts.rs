use polonius_engine::{Atom, FactTypes};
use rustc_borrowck::consumers::{LocationTable, RustcFacts};
use rustc_index::vec::IndexVec;
use rustc_middle::{
    mir::{self, BasicBlock, Local, Location},
    ty::RegionVid,
};
use rustc_mir_dataflow::move_paths::MovePathIndex;
use std::{
    io::{BufRead, BufReader},
    num::ParseIntError,
    path::Path,
};

extern crate polonius_engine;

#[derive(Debug, Copy, Clone)]
struct AlmostRustcFacts;

impl FactTypes for AlmostRustcFacts {
    type Loan = <RustcFacts as FactTypes>::Loan;
    type Origin = <RustcFacts as FactTypes>::Origin;
    type Path = <RustcFacts as FactTypes>::Loan;
    type Variable = <RustcFacts as FactTypes>::Loan;
    type Point = LocationIndexWrapper;
}

pub struct FlowistryFactSelection<F: FactTypes> {
    pub subset_base: Vec<(
        <F as FactTypes>::Origin,
        <F as FactTypes>::Origin,
        <F as FactTypes>::Point,
    )>,
}

type AlmostFlowistryFacts = FlowistryFactSelection<AlmostRustcFacts>;
pub type FlowistryFacts = FlowistryFactSelection<RustcFacts>;

pub fn load_facts_for_flowistry(
    ltab: &LocationTable,
    facts_dir: &Path,
) -> std::io::Result<FlowistryFacts> {
    let facts = AlmostFlowistryFacts {
        subset_base: {
            let facts_path = facts_dir.join("subset_base.facts");
            println!("file: {:?}", facts_path);
            let facts_file = std::fs::File::open(facts_path)?;
            parse_tab_delimited_file(ltab, BufReader::new(facts_file))?
        },
    };

    // Fucking yikes. Rustc exposes all of its fact types, *except*
    // LocationIndex, which prevents me from implementing traits for it. So I am
    // forced to wrap it (with LocationIndexWrapper) and then transmute.
    //
    // This is safe though, because the #[repr(transparent)] means the
    // representation of the wrapper is guaranteed to match that of the wrapped
    // type.
    Ok(unsafe { std::mem::transmute(facts) })
}

fn strip_delimiters(start: char, end: char, target: &str) -> Result<&str, ParseErr> {
    let a = target
        .strip_prefix(start)
        .ok_or(ParseErr::NotDelimited(start))?;
    a.strip_suffix(end).ok_or(ParseErr::NotDelimited(end))
}

fn parse_tab_delimited_file<Ctx, Row: FromTabDelimited<Ctx>>(
    ltab: &Ctx,
    reader: impl BufRead,
) -> std::io::Result<Vec<Row>> {
    reader
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line = line?;
            let mut columns = line.split('\t');
            let row = FromTabDelimited::parse(ltab, &mut columns).unwrap();

            if columns.next().is_some() {
                panic!("extra data on line {}", index + 1);
            }

            Ok(row)
        })
        .collect()
}

#[derive(Debug)]
enum ParseErr<'i> {
    PrefixNotFound(&'static str),
    SuffixNotFound(&'static str),
    NotAnInteger(ParseIntError),
    InputExhausted,
    NotDelimited(char),
    UnknownRichLocationKind(&'i str),
}

trait FromTabDelimited<Ctx>: Sized {
    fn parse<'i>(
        ctx: &Ctx,
        inputs: &mut dyn Iterator<Item = &'i str>,
    ) -> Result<Self, ParseErr<'i>>;
}

macro_rules! parse_tab_delim_tup {
    ($($id:ident),+) => {
        impl<Ctx, $($id:FromTabDelimited<Ctx>),+> FromTabDelimited<Ctx> for ($($id),+) {
            #[allow(non_snake_case)]
            fn parse<'i>(
                ctx: &Ctx,
                inputs: &mut dyn Iterator<Item = &'i str>,
            ) -> Result<Self, ParseErr<'i>> {
                $(
                    let $id = $id::parse(ctx, inputs)?;
                )+
                Ok(($($id),+))
            }
        }
    };
}

parse_tab_delim_tup!(A, B);
parse_tab_delim_tup!(A, B, C);
parse_tab_delim_tup!(A, B, C, D);
parse_tab_delim_tup!(A, B, C, D, E);
parse_tab_delim_tup!(A, B, C, D, E, F);

macro_rules! parse_tab_delim_prefix_str {
    ($t:ty, $prefix:literal, $suffix:literal) => {
        impl<Ctx> FromTabDelimited<Ctx> for $t {
            fn parse<'i>(
                _: &Ctx,
                inputs: &mut dyn Iterator<Item = &'i str>,
            ) -> Result<Self, ParseErr<'i>> {
                let s = inputs.next().ok_or(ParseErr::InputExhausted)?;
                let inner = strip_delimiters('"', '"', s)?;
                Ok(<$t>::from_u32(
                    inner
                        .strip_prefix($prefix)
                        .ok_or(ParseErr::PrefixNotFound($prefix))?
                        .strip_suffix($suffix)
                        .ok_or(ParseErr::SuffixNotFound($suffix))?
                        .parse()
                        .map_err(ParseErr::NotAnInteger)?,
                ))
            }
        }
    };
}

#[allow(dead_code)]
struct BorrowIndexHack {
    private: u32
}

impl BorrowIndexHack {
    fn from_u32(value: u32) -> Self {
        BorrowIndexHack { private: value }
    }
}

parse_tab_delim_prefix_str!(Local, "_", "");
parse_tab_delim_prefix_str!(RegionVid, "'_#", "r");
parse_tab_delim_prefix_str!(BorrowIndexHack, "bw", "");
parse_tab_delim_prefix_str!(MovePathIndex, "mp", "");

pub type LocationIndex = <RustcFacts as FactTypes>::Point;

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
struct LocationIndexWrapper(LocationIndex);

impl From<usize> for LocationIndexWrapper {
    fn from(value: usize) -> Self {
        Self(From::from(value))
    }
}

impl From<LocationIndexWrapper> for usize {
    fn from(value: LocationIndexWrapper) -> Self {
        value.0.into()
    }
}

impl Atom for LocationIndexWrapper {
    fn index(self) -> usize {
        self.0.index()
    }
}

impl FromTabDelimited<LocationTable> for LocationIndexWrapper {
    fn parse<'i>(
        ctx: &LocationTable,
        inputs: &mut dyn Iterator<Item = &'i str>,
    ) -> Result<Self, ParseErr<'i>> {
        let str = inputs.next().ok_or(ParseErr::InputExhausted)?;
        let str = strip_delimiters('"', '"', str)?;
        let (ty, loc) = str.split_once('(').ok_or(ParseErr::NotDelimited('('))?;
        let loc = loc.strip_suffix(')').ok_or(ParseErr::NotDelimited(')'))?;
        let loc = loc
            .strip_prefix("bb")
            .ok_or(ParseErr::PrefixNotFound("bb"))?;
        let (bb, stmt) = loc.split_once('[').ok_or(ParseErr::NotDelimited('['))?;
        let stmt = stmt.strip_suffix(']').ok_or(ParseErr::NotDelimited(']'))?;
        let bb = bb.parse().map_err(ParseErr::NotAnInteger)?;
        let stmt = stmt.parse().map_err(ParseErr::NotAnInteger)?;
        let loc = Location {
            block: BasicBlock::from_u32(bb),
            statement_index: stmt,
        };
        let loc = match ty {
            "Mid" => ctx.mid_index(loc),
            "Start" => ctx.start_index(loc),
            other => return Err(ParseErr::UnknownRichLocationKind(other)),
        };
        Ok(LocationIndexWrapper(loc))
    }
}

#[allow(dead_code)]
struct LocationTableHack {
    num_points: usize,
    statements_before_block: IndexVec<BasicBlock, usize>,
}

pub fn create_location_table(body: &mir::Body) -> LocationTable {
    let mut num_points = 0;
    let statements_before_block = body
        .basic_blocks
        .iter()
        .map(|block_data| {
            let v = num_points;
            num_points += (block_data.statements.len() + 1) * 2;
            v
        })
        .collect();

    let hack = LocationTableHack {
        num_points,
        statements_before_block,
    };
    // Another big yikes. EXPORT THE DAMN CONSTRUCTOR FOR THIS RUSTC!!!
    unsafe { std::mem::transmute(hack) }
}
