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
use symbolic_debuginfo::{FileFormat, Object};
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

enum JsonEscaping {
    No,
    Yes,
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

    // The `LineInfos` are sorted by `address`.
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
            .binary_search_by_key(&address, |line_info| line_info.address)
        {
            Ok(index) => Some(&self.line_infos[index]),
            Err(0) => None,
            Err(next_index) => Some(&self.line_infos[next_index - 1]),
        }
    }
}

/// Debug info for a single file.
struct FileInfo {
    /// The `FuncInfo`s are sorted by `address`.
    func_infos: Vec<FuncInfo>,
    interner: Interner,
}

impl FileInfo {
    fn func_info(&self, address: u64) -> Option<&FuncInfo> {
        match self
            .func_infos
            .binary_search_by_key(&address, |func_info| func_info.address)
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
    json_escaping: JsonEscaping,
}

impl Fixer {
    fn new(json_escaping: JsonEscaping) -> Fixer {
        Fixer {
            // Matches lines produced by MozFormatCodeAddress().
            re: Regex::new(r"^(.*#\d+: )(.+)\[(.+) \+0x([0-9A-Fa-f]+)\](.*)$").unwrap(),
            file_infos: FxHashMap::default(),
            json_escaping,
        }
    }

    fn json_escape(string: &str) -> String {
        // Do the escaping.
        let escaped = serde_json::to_string(string).unwrap();

        // Strip the quotes.
        escaped[1..escaped.len() - 1].to_string()
    }

    /// Read the data from `file_name` and construct a `FileInfo` that we can
    /// subsequently query. Return a description of the failing operation on
    /// error.
    fn build_file_info(file_name: &str) -> Result<FileInfo, String> {
        let msg = |op: &str| format!("Unable to {} `{}`", op, file_name);

        // Read the file.
        let mut data = fs::read(file_name).map_err(|_| msg("read"))?;

        // On some platforms we have to get the debug info from another file.
        // Get the name of that file, if there is one.
        let file_name2 = match Object::peek(&data) {
            FileFormat::Pe => {
                let pe_object = Object::parse(&data).map_err(|_| msg("parse"))?;
                if let Object::Pe(pe) = pe_object {
                    // PE files should contain a pointer to a PDB file.
                    let pdb_file_name = pe.debug_file_name().ok_or_else(|| msg("find PDB for"))?;
                    Some(pdb_file_name.to_string())
                } else {
                    panic!(); // Impossible: peek() said it was a PE object.
                }
            }
            _ => None,
        };
        if let Some(file_name2) = file_name2 {
            data = fs::read(&file_name2)
                .map_err(|_| msg(&format!("read debug info file `{}` for", file_name2)))?;
        }

        // Get the debug session from the file data.
        let object = Object::parse(&data).map_err(|_| msg("parse"))?;
        let debug_session = object
            .debug_session()
            .map_err(|_| msg("read debug info from"))?;

        // Build the `FileInfo` from the debug session. `tests/README.md` has an
        // explanation of the commented-out `eprintln!` statements.
        let mut interner = Interner::default();
        let mut func_infos: Vec<_> = debug_session
            .functions()
            .filter_map(|function| {
                let function = function.ok()?;
                //eprintln!(
                //    "FUNC 0x{:x} size={} func={}",
                //    function.address,
                //    function.size,
                //    function.name.as_str()
                //);
                Some(FuncInfo {
                    address: function.address,
                    size: function.size,
                    mangled_name: function.name.as_str().to_string(),
                    line_infos: function
                        .lines
                        .into_iter()
                        .map(|line| {
                            //eprintln!(
                            //    "LINE 0x{:x} line={} file={}",
                            //    line.address,
                            //    line.line,
                            //    line.file.path_str()
                            //);
                            LineInfo {
                                address: line.address,
                                line: line.line,
                                path: interner.intern(line.file.path_str()),
                            }
                        })
                        .collect(),
                })
            })
            .collect();
        func_infos.sort_unstable_by_key(|func_info| func_info.address);
        func_infos.dedup_by_key(|func_info| func_info.address);

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
        let func_name = &captures[2];
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

        let mut func_name_and_locn = if let Some(func_info) = file_info.func_info(address) {
            if let Some(line_info) = func_info.line_info(address) {
                // We have the filename and line number from the debug info.
                format!(
                    "{} ({}:{})",
                    func_info.demangled_name(),
                    file_info.interner.get(line_info.path),
                    line_info.line
                )
            } else {
                // We have the filename from the debug info, but no line number.
                format!("{} ({})", func_info.demangled_name(), file_name)
            }
        } else {
            // We have nothing from the symbols or debug info. Use the file name
            // from original input, which is probably "???". The end result is the
            // same as the original line, but with the address removed and slightly
            // different formatting.
            format!("{} ({})", func_name, file_name)
        };

        if let JsonEscaping::Yes = self.json_escaping {
            func_name_and_locn = Fixer::json_escape(&func_name_and_locn);
        }
        format!("{}{}{}", before, func_name_and_locn, after)
    }
}

#[rustfmt::skip]
const USAGE_MSG: &str =
r##"usage: fix-stacks [options] < input > output

Post-process the stack frames produced by MozFormatCodeAddress().

options:
  -h, --help      show this message and exit
  -j, --json      Use JSON escaping for printed function names and file names
"##;

fn main_inner() -> io::Result<()> {
    // Process command line arguments.
    let mut json_escaping = JsonEscaping::No;
    for arg in env::args().skip(1) {
        if arg == "-h" || arg == "--help" {
            println!("{}", USAGE_MSG);
            return Ok(());
        } else if arg == "-j" || arg == "--json" {
            json_escaping = JsonEscaping::Yes;
        } else {
            let msg = format!(
                "bad argument `{}`. Run `fix-stacks -h` for more information.",
                arg
            );
            return Err(io::Error::new(io::ErrorKind::Other, msg));
        }
    }

    let reader = io::BufReader::new(io::stdin());

    let mut fixer = Fixer::new(json_escaping);
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
