#![allow(unused_variables)]
use std::{borrow::Cow, ops::Range, rc::Rc};

use gimli::{EndianRcSlice, LittleEndian};
use object::{File, Object, ObjectSection};

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

    #[allow(dead_code)]
    pub fn get_function_from_pc(&self, pc: u64) -> Result<Option<Range<u64>>, gimli::Error> {
        let loc = self.ctx.find_location_range(pc, pc + 1)?.next();
        match loc {
            Some((low, high, _)) => Ok(Some(low..high)),
            None => Ok(None),
        }
    }

    pub fn source_for_pc(&self, pc: u64) -> Result<Option<addr2line::Location>, gimli::Error> {
        self.ctx.find_location(pc)
    }
}
