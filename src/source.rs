use std::{
    fs::File,
    io::{self, BufRead},
    path::Path,
};

/// Print source code

fn try_print_source<P: AsRef<Path>>(path: P, line: usize, context: usize) -> Result<(), io::Error> {
    let path = path.as_ref();
    let lineno = line as isize;
    let context = context as isize;
    let mut curr: isize = 1;
    let f = File::open(path)?;
    let f = io::BufReader::new(f);
    println!("{}:", path.display());
    for line in f.lines() {
        let line = line?;
        if lineno - context <= curr && curr <= lineno + context {
            let cursor = if lineno == curr { ">" } else { " " };
            println!("{}  {}", cursor, line);
        }
        if curr > lineno + context {
            break;
        }
        curr += 1;
    }
    Ok(())
}

pub fn print_source<P: AsRef<Path>>(path: P, line: usize, context: usize) {
    let path = path.as_ref();
    if let Err(err) = try_print_source(path, line, context) {
        eprintln!("could not print source from {}: {}", path.display(), err);
    }
}

pub fn print_source_loc(loc: &addr2line::Location, context: usize) {
    let path = loc.file.unwrap();
    let line = loc.line.unwrap() as usize;
    print_source(path, line, context);
}
