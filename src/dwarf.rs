//! Get debug info from the binary's DWARF information.
//!
//! The implementation here is based on a combination of using addr2line where
//! possible, using the libelfin source to figure out how to use gimli, and the
//! [simple gimli
//! example](https://github.com/gimli-rs/gimli/blob/master/examples/simple.rs)
//! to understand how iteration works. The `addr2line` source code was also
//! extremely useful for understanding gimli.

#![allow(unused_variables)]
use std::{borrow::Cow, fmt, ops::Range, rc::Rc};

use addr2line::Location;
use gimli::{
    AttributeValue, BaseAddresses, CfaRule, DebuggingInformationEntry, Dwarf, EhFrame,
    EndianRcSlice, EndianSlice, LittleEndian, Reader, Register, RegisterRule, Unit, UnwindContext,
    UnwindSection,
};
use object::{Object, ObjectSection, ObjectSymbol, SymbolKind};

/// Identify the type of a symbol.
///
/// These names match the ELF standard.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SymbolType {
    NoType,
    Object,
    Func,
    Section,
    File,
}

impl fmt::Display for SymbolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolType::NoType => "notype".fmt(f),
            SymbolType::Object => "object".fmt(f),
            SymbolType::Func => "func".fmt(f),
            SymbolType::Section => "section".fmt(f),
            SymbolType::File => "file".fmt(f),
        }
    }
}

/// Convert a generic object::SymbolKind to a SymbolType, mapping back to the
/// ELF names.
impl TryFrom<SymbolKind> for SymbolType {
    type Error = String;
    fn try_from(value: SymbolKind) -> Result<Self, Self::Error> {
        use SymbolType::*;
        match value {
            SymbolKind::Null => Ok(NoType),
            SymbolKind::Text => Ok(Func),
            SymbolKind::Data => Ok(Object),
            SymbolKind::Section => Ok(Section),
            SymbolKind::File => Ok(File),
            SymbolKind::Label => Err("unexpected label symbol".to_string()),
            SymbolKind::Tls => Err("unsupported tls symbol".to_string()),
            SymbolKind::Unknown => Ok(NoType),
            _ => Err("other symbol kind".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub type_: SymbolType,
    pub name: String,
    pub addr: u64,
}

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

// the gimli::Reader we use
type R = EndianRcSlice<LittleEndian>;

struct UnwindInfo {
    addr: u64,
    eh_data: Vec<u8>,
}

pub struct DbgInfo<'data> {
    /// underlying object file
    file: object::File<'data>,
    unwind: UnwindInfo,
    /// context for doing offset -> source lookups
    ctx: addr2line::Context<R>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnAddrRule<'a> {
    pub cfa: CfaRule<EndianSlice<'a, LittleEndian>>,
    pub ra: RegisterRule<EndianSlice<'a, LittleEndian>>,
}

impl<'data> DbgInfo<'data> {
    pub fn new(file: object::File<'data>) -> gimli::Result<Self> {
        let load_section = |id: gimli::SectionId| -> Result<R, gimli::Error> {
            let data = file
                .section_by_name(id.name())
                .and_then(|section| section.uncompressed_data().ok())
                .unwrap_or(Cow::Borrowed(&[][..]));
            // NOTE: I believe this actually copies all of the data for each
            // section in order to initialize the Rc, that's why the R here is
            // fully owned.
            Ok(R::new(Rc::from(&*data), LittleEndian))
        };

        // Load all of the sections.
        let dwarf = gimli::Dwarf::load(&load_section)?;
        let ctx = addr2line::Context::from_dwarf(dwarf)?;
        let (eh_frame_addr, eh_frame_data) = file
            .section_by_name(gimli::SectionId::EhFrame.name())
            .and_then(|section| {
                let addr = section.address();
                section
                    .uncompressed_data()
                    .ok()
                    .map(|data| (addr, data.to_vec()))
            })
            .unwrap_or_default();
        Ok(Self {
            file,
            unwind: UnwindInfo {
                addr: eh_frame_addr,
                eh_data: eh_frame_data,
            },
            ctx,
        })
    }

    fn dwarf(&self) -> &Dwarf<R> {
        self.ctx.dwarf()
    }

    fn eh_frame(&self) -> EhFrame<EndianSlice<'_, LittleEndian>> {
        EhFrame::new(&self.unwind.eh_data, LittleEndian)
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
        let dwarf = self.dwarf();
        let mut units = dwarf.units();
        while let Some(header) = units.next()? {
            let unit = dwarf.unit(header)?;
            let mut rows = match unit.line_program.clone() {
                Some(ilnp) => ilnp.rows(),
                None => continue,
            };
            while let Some((header, row)) = rows.next_row()? {
                // is_stmt marks the instructions the compiler thinks are the
                // best places for a breakpoint
                if !row.is_stmt() {
                    continue;
                }
                // TODO: could cache these checks based on the row.file_index()
                match row.file(header) {
                    None => continue,
                    Some(fe) => {
                        let file = dwarf.attr_string(&unit, fe.path_name())?;
                        if !file_pred(&file.to_string()?) {
                            continue;
                        }
                    }
                }
                // file matches, now check line number
                if let Some(this_line) = row.line() {
                    if this_line.get() as usize == line {
                        return Ok(Some(row.address()));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Find a symbol in the symbol table by name, gathering any matches
    pub fn lookup_symbol(&self, name: &str) -> Vec<Symbol> {
        let needle = name;
        self.file
            .symbols()
            .filter_map(|sym| {
                sym.name().ok().and_then(|name| {
                    let name = addr2line::demangle(name, gimli::DW_LANG_Rust)
                        .unwrap_or_else(|| name.to_string());
                    if name != needle {
                        return None;
                    }
                    // found a matching symbol
                    sym.kind().try_into().ok().map(|type_| Symbol {
                        type_,
                        name,
                        addr: sym.address(),
                    })
                })
            })
            .collect()
    }

    /// Get the debug info on the return address from a particular pc.
    ///
    /// Returns only the information on how to get the return address, not the actual value.
    pub fn get_unwind_return_address(&self, pc: u64) -> gimli::Result<Option<ReturnAddrRule>> {
        let eh_frame = self.eh_frame();
        let bases = BaseAddresses::default().set_eh_frame(self.unwind.addr);
        let mut ctx = UnwindContext::new();
        let info = match eh_frame.unwind_info_for_address(
            &bases,
            &mut ctx,
            pc,
            EhFrame::cie_from_offset,
        ) {
            Ok(info) => info,
            Err(gimli::Error::NoUnwindInfoForAddress) => return Ok(None),
            Err(err) => return Err(err),
        };
        let cfa = info.cfa();
        // 16 is the return address dwarf register number (at least for x86-64)
        let ra = info.register(Register(16));
        Ok(Some(ReturnAddrRule {
            cfa: cfa.clone(),
            ra,
        }))
    }
}
