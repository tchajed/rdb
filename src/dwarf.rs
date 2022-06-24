#![allow(unused_variables)]
use std::{borrow::Cow, ops::Range, rc::Rc};

use addr2line::Location;
use gimli::{AttributeValue, DebuggingInformationEntry, EndianRcSlice, LittleEndian};
use object::{File, Object, ObjectSection};

type Die<'abbrev, 'unit, R> =
    DebuggingInformationEntry<'abbrev, 'unit, R, <R as gimli::Reader>::Offset>;

fn at_low_pc<R: gimli::Reader>(die: &Die<R>) -> gimli::Result<u64> {
    let low_pc = die
        .attr(gimli::DW_AT_low_pc)?
        .map(|attr| {
            if let AttributeValue::Addr(a) = attr.value() {
                a
            } else {
                panic!("invalid low pc type")
            }
        })
        .unwrap_or(0);
    Ok(low_pc)
}

fn at_high_pc(low_pc: u64, high_pc: AttributeValue<impl gimli::Reader>) -> u64 {
    match high_pc {
        AttributeValue::Addr(a) => a,
        AttributeValue::Sdata(n) => low_pc + n as u64,
        AttributeValue::Udata(n) => low_pc + n as u64,
        _ => panic!("invalid high_pc type"),
    }
}

fn at_pc_range(die: &Die<impl gimli::Reader>) -> gimli::Result<Range<u64>> {
    let low_pc = at_low_pc(die)?;
    let high_pc = die
        .attr(gimli::DW_AT_high_pc)?
        .map(|a| at_high_pc(low_pc, a.value()))
        .unwrap_or(low_pc + 1);
    Ok(low_pc..high_pc)
}

// fn unit_pc_range(unit: &Unit<impl gimli::Reader>) -> gimli::Result<Range<u64>> {
//     at_pc_range(unit.entries_tree(None)?.root()?.entry())
// }

// the gimli::Reader we use
type Reader = EndianRcSlice<LittleEndian>;

pub struct DbgInfo {
    // an addr2line context
    ctx: addr2line::Context<Reader>,
}

impl DbgInfo {
    pub fn new(file: &File) -> gimli::Result<Self> {
        let load_section = |id: gimli::SectionId| -> Result<Reader, gimli::Error> {
            let data = file
                .section_by_name(id.name())
                .and_then(|section| section.uncompressed_data().ok())
                .unwrap_or(Cow::Borrowed(&[][..]));
            Ok(Reader::new(Rc::from(&*data), LittleEndian))
        };

        // Load all of the sections.
        let dwarf = gimli::Dwarf::load(&load_section)?;
        let ctx = addr2line::Context::from_dwarf(dwarf)?;
        Ok(Self { ctx })
    }

    fn get_function_range_from_pc(&self, pc: u64) -> Result<Option<Range<u64>>, gimli::Error> {
        let unit = match self.ctx.find_dwarf_unit(pc) {
            Some(unit) => unit,
            None => return Ok(None),
        };
        // Iterate over the Debugging Information Entries (DIEs) in the unit.
        let mut depth = 0;
        let mut entries = unit.entries();

        while let Some((delta_depth, entry)) = entries.next_dfs()? {
            depth += delta_depth;
            if entry.tag() != gimli::DW_TAG_subprogram {
                continue;
            }
            let range = at_pc_range(entry)?;
            if range.contains(&pc) {
                return Ok(Some(range));
            }
        }
        Ok(None)
    }

    pub fn function_lines_from_pc(&self, pc: u64) -> Result<Vec<(u64, u64)>, gimli::Error> {
        let range = match self.get_function_range_from_pc(pc)? {
            Some(range) => range,
            None => return Ok(vec![]),
        };
        let mut locs = vec![];
        let iter = self.ctx.find_location_range(range.start, range.end)?;
        for (start, end, loc) in iter {
            locs.push((start, end));
        }
        Ok(locs)
    }

    pub fn source_for_pc(&self, pc: u64) -> Result<Option<Location>, gimli::Error> {
        self.ctx.find_location(pc)
    }
}
