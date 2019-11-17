// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use fxhash::FxHashMap;
use regex::Regex;
use std::collections::hash_map::Entry;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use symbolic_common::Name;
use symbolic_debuginfo::Object;
use symbolic_demangle::{Demangle, DemangleFormat, DemangleOptions};

#[cfg(test)]
mod tests;

/// An interned string type. Many file paths are repeated, so having this type
/// reduces peak memory usage significantly.
#[derive(Clone, Copy)]
struct InternedString(usize);

/// A simple string interner. Each string is sub-optimally stored twice, but
/// there is so much repetition relative to the number of distinct strings that
/// this barely matters, and doing better would require use of `unsafe`.
#[derive(Default)]
struct Interner {
    map: FxHashMap<String, usize>,
    strings: Vec<String>,
}

impl Interner {
    fn intern(&mut self, string: String) -> InternedString {
        let index = if let Some(&index) = self.map.get(&string) {
            index
        } else {
            let index = self.strings.len();
            self.map.insert(string.clone(), index);
            self.strings.push(string);
            index
        };
        InternedString(index)
    }

    pub fn get(&self, interned_string: InternedString) -> &str {
        &self.strings[interned_string.0]
    }
}

/// Debug info for a single line.
struct LineInfo {
    address: u64,
    line: u64,

    /// We use `InternedString` here because paths are often duplicated.
    path: InternedString,
}

/// Debug info for a single function.
struct FuncInfo {
    address: u64,
    size: u64,

    // We don't use `InternedString` here because function names are rarely
    // duplicated.
    mangled_name: String,

    line_infos: Box<[LineInfo]>,
}

impl FuncInfo {
    fn demangled_name(&self) -> String {
        let options = DemangleOptions {
            format: DemangleFormat::Full,
            with_arguments: true,
        };
        Name::new(self.mangled_name.as_str())
            .try_demangle(options)
            .to_string()
    }

    fn contains(&self, address: u64) -> bool {
        self.address <= address && address < self.address + self.size
    }

    fn line_info(&self, address: u64) -> Option<&LineInfo> {
        match self
            .line_infos
            .binary_search_by_key(&address, |li| li.address)
        {
            Ok(index) => Some(&self.line_infos[index]),
            Err(0) => None,
            Err(next_index) => Some(&self.line_infos[next_index - 1]),
        }
    }
}

/// Debug info for a single file.
struct FileInfo {
    func_infos: Box<[FuncInfo]>,
    interner: Interner,
}

impl FileInfo {
    fn func_info(&self, address: u64) -> Option<&FuncInfo> {
        match self
            .func_infos
            .binary_search_by_key(&address, { |func_info| func_info.address })
        {
            Ok(index) => Some(&self.func_infos[index]),
            Err(0) => None,
            Err(next_index) => {
                let func_info = &self.func_infos[next_index - 1];
                if func_info.contains(address) {
                    Some(func_info)
                } else {
                    None
                }
            }
        }
    }
}

/// The top level structure that does the work.
struct Fixer {
    re: Regex,
    file_infos: FxHashMap<String, FileInfo>,
}

impl Fixer {
    fn new() -> Fixer {
        Fixer {
            // Matches lines produced by MozFormatCodeAddress().
            re: Regex::new(r"^(.*#\d+: )(.+)\[(.+) \+0x([0-9A-Fa-f]+)\](.*)$").unwrap(),
            file_infos: FxHashMap::default(),
        }
    }

    /// Read the data from `file_name` and construct a `FileInfo` that we can
    /// subsequently query. Return a description of the failing operation on
    /// error.
    fn build_file_info(file_name: &str) -> Result<FileInfo, String> {
        // Get the debug session from file.
        let msg = |op| format!("Unable to {} file {}", op, file_name);
        let data = fs::read(file_name).map_err(|_| msg("read"))?;
        let object = Object::parse(&data).map_err(|_| msg("parse"))?;
        let debug_session = object
            .debug_session()
            .map_err(|_| msg("read debug info from"))?;

        // Build the `FileInfo` from the debug session.
        let mut interner = Interner::default();
        let func_infos = debug_session
            .functions()
            .filter_map(|function| {
                let function = function.ok()?;
                Some(FuncInfo {
                    address: function.address,
                    size: function.size,
                    mangled_name: function.name.as_str().to_string(),
                    line_infos: function
                        .lines
                        .into_iter()
                        .map(|line| LineInfo {
                            address: line.address,
                            line: line.line,
                            path: interner.intern(line.file.path_str()),
                        })
                        .collect(),
                })
            })
            .collect();

        let file_info = FileInfo {
            func_infos,
            interner,
        };

        Ok(file_info)
    }

    /// Fix stack frames within `line` as necessary. Prints any errors to stderr.
    #[inline]
    fn fix(&mut self, line: String) -> String {
        // Apply the regexp.
        let captures = if let Some(captures) = self.re.captures(&line) {
            captures
        } else {
            return line;
        };

        let before = &captures[1];
        let fn_name = &captures[2];
        let file_name = &captures[3];
        let address = u64::from_str_radix(&captures[4], 16).unwrap();
        let after = &captures[5];

        // If we haven't seen this file yet, parse and record its contents, for
        // this lookup and any future lookups.
        let file_info = match self.file_infos.entry(file_name.to_string()) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => match Fixer::build_file_info(file_name) {
                Ok(file_info) => v.insert(file_info),
                Err(msg) => {
                    eprintln!("{}", msg);
                    return line;
                }
            },
        };

        if let Some(func_info) = file_info.func_info(address) {
            let name = func_info.demangled_name();
            if let Some(line_info) = func_info.line_info(address) {
                // We have the filename and line number from the debug info.
                let path = file_info.interner.get(line_info.path);
                format!("{}{} ({}:{}){}", before, name, path, line_info.line, after)
            } else {
                // We have the filename from the debug info, but no line number.
                format!("{}{} ({}){}", before, name, file_name, after)
            }
        } else {
            // We have nothing from the symbols or debug info. Use the file name
            // from original input, which is probably "???". The end result is the
            // same as the original line, but with the address removed and slightly
            // different formatting.
            format!("{}{} ({}){}", before, fn_name, file_name, after)
        }
    }
}

#[rustfmt::skip]
const USAGE_MSG: &str =
r##"usage: fix-stacks [options] < input > output

Post-process the stack frames produced by MozFormatCodeAddress().

options:
  -h, --help      show this message and exit
"##;

fn main_inner() -> io::Result<()> {
    // Process command line arguments.
    for arg in env::args().skip(1) {
        if arg == "-h" || arg == "--help" {
            println!("{}", USAGE_MSG);
            return Ok(());
        } else {
            let msg = format!(
                "bad argument `{}`. Run `fix-stacks -h` for more information.",
                arg
            );
            return Err(io::Error::new(io::ErrorKind::Other, msg));
        }
    }

    let reader = io::BufReader::new(io::stdin());

    let mut fixer = Fixer::new();
    for line in reader.lines() {
        writeln!(io::stdout(), "{}", fixer.fix(line.unwrap()))?;
    }

    Ok(())
}

fn main() {
    // Ignore broken pipes, e.g. when piping output through `head -10`.
    if let Err(err) = main_inner() {
        if err.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("fix-stacks: {}", err);
            std::process::exit(1);
        }
    }
}
