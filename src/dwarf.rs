#![allow(unused_variables)]
use std::{borrow::Cow, ops::Range, rc::Rc};

use addr2line::Location;
use gimli::{
    AttributeValue, DebuggingInformationEntry, Dwarf, EndianRcSlice, LittleEndian, Reader, Unit,
};
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
type R = EndianRcSlice<LittleEndian>;

pub struct DbgInfo {
    // an addr2line context
    ctx: addr2line::Context<R>,
}

impl DbgInfo {
    pub fn new(file: &File) -> gimli::Result<Self> {
        let load_section = |id: gimli::SectionId| -> Result<R, gimli::Error> {
            let data = file
                .section_by_name(id.name())
                .and_then(|section| section.uncompressed_data().ok())
                .unwrap_or(Cow::Borrowed(&[][..]));
            Ok(R::new(Rc::from(&*data), LittleEndian))
        };

        // Load all of the sections.
        let dwarf = gimli::Dwarf::load(&load_section)?;
        let ctx = addr2line::Context::from_dwarf(dwarf)?;
        Ok(Self { ctx })
    }

    fn dwarf(&self) -> &Dwarf<R> {
        self.ctx.dwarf()
    }

    fn at_name<'a>(&self, unit: &'a Unit<R>, die: &'a Die<R>) -> gimli::Result<Option<R>> {
        let attr = match die.attr(gimli::DW_AT_name)? {
            Some(attr) => attr,
            None => return Ok(None),
        };
        let val = self.dwarf().attr_string(unit, attr.value())?;
        Ok(Some(val))
    }

    fn get_function_range_from_pc(&self, pc: u64) -> Result<Option<Range<u64>>, gimli::Error> {
        let unit = match self.ctx.find_dwarf_unit(pc) {
            Some(unit) => unit,
            None => return Ok(None),
        };

        // Iterate over the Debugging Information Entries (DIEs) in the unit.
        let mut entries = unit.entries();
        while let Some((_, entry)) = entries.next_dfs()? {
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

    pub fn function_lines_from_pc(&self, pc: u64) -> Result<Vec<u64>, gimli::Error> {
        let range = match self.get_function_range_from_pc(pc)? {
            Some(range) => range,
            None => return Ok(vec![]),
        };
        let mut locs = vec![];
        let iter = self.ctx.find_location_range(range.start, range.end)?;
        for (start, end, loc) in iter {
            locs.push(start);
        }
        Ok(locs)
    }

    pub fn source_for_pc(&self, pc: u64) -> Result<Option<Location>, gimli::Error> {
        self.ctx.find_location(pc)
    }

    pub fn pc_for_function_pred<F>(&self, pred: F) -> Result<Option<u64>, gimli::Error>
    where
        F: for<'a> Fn(&'a str) -> bool,
    {
        let mut units = self.ctx.dwarf().units();
        while let Some(header) = units.next()? {
            let unit = self.ctx.dwarf().unit(header)?;

            let mut entries = unit.entries();
            while let Some((_, entry)) = entries.next_dfs()? {
                let name = match self.at_name(&unit, entry)? {
                    Some(name) => name,
                    None => continue,
                };
                if pred(&name.to_string()?) {
                    let pc = at_low_pc(entry)?;
                    return Ok(Some(pc));
                }
            }
        }
        Ok(None)
    }

    pub fn pc_for_source_loc<F>(
        &self,
        file_pred: F,
        line: usize,
    ) -> Result<Option<u64>, gimli::Error>
    where
        F: for<'a> Fn(&'a str) -> bool,
    {
        let mut units = self.ctx.dwarf().units();
        while let Some(header) = units.next()? {
            let unit = self.ctx.dwarf().unit(header)?;
            let mut rows = match unit.line_program.clone() {
                Some(ilnp) => ilnp.rows(),
                None => continue,
            };
            while let Some((header, row)) = rows.next_row()? {
                if !row.is_stmt() {
                    continue;
                }
                // TODO: could cache these checks based on the row.file_index()
                if let Some(fe) = row.file(header) {
                    let file = self.dwarf().attr_string(&unit, fe.path_name())?;
                    let file = file.to_string()?;
                    if !file_pred(&file) {
                        continue;
                    }
                } else {
                    continue;
                }
                if let Some(this_line) = row.line() {
                    if this_line.get() as usize == line {
                        return Ok(Some(row.address()));
                    }
                }
            }
        }
        Ok(None)
    }
}
