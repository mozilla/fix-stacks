// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use fxhash::{FxHashMap, FxHashSet};
use goblin::{archive, mach};
use regex::Regex;
use std::collections::hash_map::Entry;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use symbolic_common::{Arch, Name};
use symbolic_debuginfo::{Archive, FileFormat, Function, Object, ObjectDebugSession};
use symbolic_demangle::{Demangle, DemangleFormat, DemangleOptions};

#[cfg(test)]
mod tests;

/// Should debugging output for functions and lines be printed? (See
/// `tests/README.md` for more details.)
const PRINT_FUNCS_AND_LINES: bool = false;

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

fn format_address(address: u64, offset: i64) -> String {
    if offset == 0 {
        format!("0x{:x}", address)
    } else {
        format!(
            "0x{:x} -> 0x{:x}",
            address,
            (address as i64 + offset) as u64
        )
    }
}

/// Debug info for a single line.
struct LineInfo {
    address: u64,
    line: u64,

    /// We use `InternedString` here because paths are often duplicated.
    path: InternedString,
}

impl LineInfo {
    fn new(interner: &mut Interner, line: symbolic_debuginfo::LineInfo, offset: i64) -> LineInfo {
        if PRINT_FUNCS_AND_LINES {
            eprintln!(
                "LINE {} line={} file={}",
                format_address(line.address, offset),
                line.line,
                line.file.path_str()
            );
        }
        LineInfo {
            address: (line.address as i64 + offset) as u64,
            line: line.line,
            path: interner.intern(line.file.path_str()),
        }
    }
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
    fn new(interner: &mut Interner, function: Function, offset: i64) -> FuncInfo {
        if PRINT_FUNCS_AND_LINES {
            eprintln!(
                "FUNC {} size={} func={}",
                format_address(function.address, offset),
                function.size,
                function.name.as_str()
            );
        }
        FuncInfo {
            address: (function.address as i64 + offset) as u64,
            size: function.size,
            mangled_name: function.name.as_str().to_string(),
            line_infos: function
                .lines
                .into_iter()
                .map(|line| LineInfo::new(interner, line, offset))
                .collect(),
        }
    }

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
#[derive(Default)]
struct FileInfo {
    interner: Interner,

    /// The `FuncInfo`s are sorted by `address`.
    func_infos: Vec<FuncInfo>,
}

impl FileInfo {
    fn new(debug_session: ObjectDebugSession) -> FileInfo {
        // Build the `FileInfo` from the debug session.
        let mut interner = Interner::default();
        let mut func_infos: Vec<_> = debug_session
            .functions()
            .filter_map(|function| {
                let function = function.ok()?;
                Some(FuncInfo::new(&mut interner, function, 0))
            })
            .collect();
        func_infos.sort_unstable_by_key(|func_info| func_info.address);
        func_infos.dedup_by_key(|func_info| func_info.address);

        FileInfo {
            interner,
            func_infos,
        }
    }

    /// Add the debug info from `debug_session` for functions in `filename`
    /// to that already gathered in `interner` and `func_infos`, but only for
    /// functions present in `sym_func_addrs`.
    fn add(
        sym_func_addrs: &SymFuncAddrs,
        file_name: &str,
        debug_session: ObjectDebugSession,
        interner: &mut Interner,
        func_infos: &mut Vec<FuncInfo>,
    ) {
        // Build the `FileInfo` from the debug session.
        func_infos.extend(debug_session.functions().filter_map(|function| {
            let function = function.ok()?;

            // If a function appears in the debug info but was not seen in
            // the parent binary's symbol table, just ignore it. This is
            // common, perhaps due to inlining (i.e. inlined functions don't
            // end up with their own symbols in the binary).
            //
            // Otherwise, we know the function's address in the parent binary's
            // symbol table. Adjust all the addresses from the debug info to
            // match that address.
            let sym_func_key = Fixer::sym_func_key(file_name, function.name.as_str());
            let sym_func_addr = sym_func_addrs.get(&sym_func_key)?;
            let offset = *sym_func_addr as i64 - function.address as i64;
            Some(FuncInfo::new(interner, function, offset))
        }));
    }

    /// Finish constructing a `FileInfo` that has been built up using
    /// `Fixer::add`.
    fn finish(interner: Interner, mut func_infos: Vec<FuncInfo>) -> FileInfo {
        func_infos.sort_unstable_by_key(|func_info| func_info.address);
        func_infos.dedup_by_key(|func_info| func_info.address);

        FileInfo {
            func_infos,
            interner,
        }
    }

    /// Get the `FuncInfo` for an address, if there is one.
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

/// Records address of functions from a symbol table.
type SymFuncAddrs = FxHashMap<String, u64>;

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
        let data = fs::read(file_name).map_err(|_| "read")?;
        let file_format = Archive::peek(&data);
        match file_format {
            FileFormat::Elf => Fixer::build_file_info_direct(&data),
            FileFormat::Pe => Fixer::build_file_info_pe(&data),
            FileFormat::Pdb => Fixer::build_file_info_direct(&data),
            FileFormat::MachO => Fixer::build_file_info_macho(&data),
            _ => Err(format!("parse {} format file", file_format)),
        }
    }

    // "Direct" means that the debug info is within `data`, as opposed to being
    // in another file that `data` refers to.
    fn build_file_info_direct(data: &[u8]) -> Result<FileInfo, String> {
        let object = Object::parse(&data).map_err(|_| "parse")?;
        let debug_session = object.debug_session().map_err(|_| "read debug info from")?;
        Ok(FileInfo::new(debug_session))
    }

    fn build_file_info_pe(data: &[u8]) -> Result<FileInfo, String> {
        // For PEs we get the debug info from a PDB file.
        let pe_object = Object::parse(&data).map_err(|_| "parse")?;
        let pe = match pe_object {
            Object::Pe(pe) => pe,
            _ => unreachable!(),
        };
        let pdb_file_name = pe.debug_file_name().ok_or("find debug info file for")?;
        let data = fs::read(pdb_file_name.to_string())
            .map_err(|_| format!("read debug info file `{}` for", pdb_file_name))?;
        Fixer::build_file_info_direct(&data)
    }

    fn build_file_info_macho(data: &[u8]) -> Result<FileInfo, String> {
        // On Mac, debug info is typically stored in `.dSYM` directories. But
        // they aren't normally built for Firefox because doing so is slow.
        // Instead, we read the symbol table of the given file, which has
        // pointers to all the object files from which it was constructed. We
        // then obtain the debug info from those object files (some of which
        // are embedded within `.a` files), and adjust the addresses from the
        // debug info appropriately. All this requires the object files to
        // still be present, and matches what `atos` does.
        //
        // Doing all this requires a lower level of processing than what the
        // `symbolic` crate provides, so instead we use the `goblin` crate.
        //
        // We stop if any errors are encountered. The code could be made more
        // robust in the face of errors if necessary.

        let macho = Fixer::macho(&data)?;
        let sym_func_addrs = Fixer::sym_func_addrs(&macho)?;

        // Iterate again through the symbol table, reading every object file
        // that is referenced, and adjusting the addresses in those files using
        // the function addresses obtained above.
        let mut seen_archives = FxHashSet::default();
        let mut func_infos = vec![];
        let mut interner = Interner::default();
        for sym in macho.symbols() {
            let (oso_name, nlist) = sym.map_err(|_| "read symbol table from")?;
            if nlist.is_stab() && nlist.n_type == mach::symbols::N_OSO {
                if let Some(ar_file_name) = Fixer::is_within_archive(oso_name) {
                    // It's an archive entry, e.g. "libgkrust.a(foo.o)". Read
                    // every entry in archive, if we haven't already done so.
                    if seen_archives.insert(ar_file_name) {
                        let ar_data = fs::read(ar_file_name)
                            .map_err(|_| format!("read ar `{}` referenced by", ar_file_name))?;
                        let ar = archive::Archive::parse(&ar_data)
                            .map_err(|_| format!("parse ar `{}` referenced by", ar_file_name))?;

                        for (name, _, _) in ar.summarize() {
                            let data = ar.extract(name, &ar_data).map_err(|_| {
                                format!("read an entry in ar `{}` referenced by", ar_file_name)
                            })?;
                            Fixer::do_macho_oso(
                                &sym_func_addrs,
                                ar_file_name,
                                data,
                                &mut interner,
                                &mut func_infos,
                            )?;
                        }
                    }
                } else {
                    // It's a normal object file. Read it.
                    let data = fs::read(oso_name)
                        .map_err(|_| format!("read `{}` referenced by", oso_name))?;
                    Fixer::do_macho_oso(
                        &sym_func_addrs,
                        oso_name,
                        &data,
                        &mut interner,
                        &mut func_infos,
                    )?;
                }
            }
        }

        Ok(FileInfo::finish(interner, func_infos))
    }

    fn macho(data: &[u8]) -> Result<mach::MachO, String> {
        let mach = mach::Mach::parse(&data).map_err(|_| "parse (with goblin)")?;
        match mach {
            mach::Mach::Binary(macho) => Ok(macho),
            mach::Mach::Fat(multi_arch) => {
                // Get the x86-64 object from the fat binary. (On Mac, Firefox
                // is only available on x86-64.)
                let macho = multi_arch.into_iter().find(|macho| {
                    if let Ok(macho) = macho {
                        if macho.header.cputype() == mach::constants::cputype::CPU_TYPE_X86_64 {
                            return true;
                        }
                    }
                    false
                });

                // This chaining is necessary because `MachOIterator::Item` is
                // not `MachO` but `Result<MachO>`. We don't distinguish
                // between the "couldn't find the x86-64 code" case and the
                // "found it, but it had an error" case.
                let msg = "find x86-64 code in the fat binary";
                macho.ok_or(msg)?.map_err(|_| msg.to_string())
            }
        }
    }

    /// Iterate through the symbol table, getting the address of every function
    /// in the file.
    fn sym_func_addrs(macho: &mach::MachO) -> Result<SymFuncAddrs, String> {
        let object_load_address = Fixer::object_load_address(macho);
        let mut sym_func_addrs = FxHashMap::default();
        let mut curr_oso_name = String::new();
        for sym in macho.symbols() {
            let (name, nlist) = sym.map_err(|_| "read symbol table from")?;
            if !nlist.is_stab() {
                continue;
            }

            if nlist.n_type == mach::symbols::N_OSO {
                // Record this reference to an object file (or archive).
                curr_oso_name = if let Some(ar_file_name) = Fixer::is_within_archive(name) {
                    // We have to strip the archive suffix, because the suffix
                    // in the symbol table often disagrees with the suffix in
                    // the debug info. E.g.
                    // - symbol table: `libjs_static.a(Unified_cpp_js_src9.o)`
                    // - debug info: `libjs_static.a(RegExp.o)`
                    //
                    // It's unclear why this occurs, though it doesn't seem to
                    // matter much, though see the comment about duplicates
                    // below.
                    ar_file_name.to_string()
                } else {
                    name.to_string()
                }
            } else if nlist.n_type == mach::symbols::N_FUN
                && nlist.n_sect != mach::symbols::NO_SECT as usize
            {
                let name = &name[1..]; // Trim the leading underscore.
                let address = nlist.n_value - object_load_address;

                let sym_func_key = Fixer::sym_func_key(&curr_oso_name, name);

                // There can be duplicates, in which case the last one "wins".
                // This seems to only occur due to the removal of archive
                // suffixes above. In practice it doesn't seem to matter.
                sym_func_addrs.insert(sym_func_key, address);
            }
        }

        Ok(sym_func_addrs)
    }

    // This is based on `symbolic_debuginfo::macho:MachObject::load_address`.
    // We need to define it because `goblin` doesn't have an equivalent, and we
    // use `goblin` rather than `symbolic_debuginfo` for Mach-O binaries.
    fn object_load_address(macho: &mach::MachO) -> u64 {
        for seg in macho.segments.iter() {
            if let Ok(name) = seg.name() {
                if name == "__TEXT" {
                    return seg.vmaddr;
                }
            }
        }
        0
    }

    /// Construct a key for the `sym_func_addrs` hash map.
    fn sym_func_key(file_name: &str, func_name: &str) -> String {
        format!("{}:{}", file_name, func_name)
    }

    /// Is this filename within an archive? E.g. `libfoo.a(bar.o)` means that
    /// `bar.o` is within the archive `libfoo.a`. If so, return the archive
    /// name.
    fn is_within_archive(file_name: &str) -> Option<&str> {
        if let (Some(index), true) = (file_name.find(".a("), file_name.ends_with(')')) {
            let ar_file_name = &file_name[..index + 2];
            Some(ar_file_name)
        } else {
            None
        }
    }

    /// Read the debug info from a file referenced by an OSO entry in a Macho-O
    /// symbol table.
    fn do_macho_oso(
        sym_func_addrs: &SymFuncAddrs,
        file_name: &str,
        data: &[u8],
        interner: &mut Interner,
        func_infos: &mut Vec<FuncInfo>,
    ) -> Result<(), String> {
        // Although we use `goblin` to iterate through the symbol
        // table, we use `symbolic` to read the debug info from the
        // object/archive, because it's easier to use.
        let archive = Archive::parse(&data)
            .map_err(|e| format!("({:?}) parse `{}` referenced by", e, file_name))?;

        // Get the x86-64 object from the archive, which might be a fat binary.
        // (On Mac, Firefox is only available on x86-64.)
        let mut x86_64_object = None;
        for object in archive.objects() {
            let object = object
                .map_err(|_| format!("parse fat binary entry in `{}` referenced by", file_name))?;
            if object.arch() == Arch::Amd64 {
                x86_64_object = Some(object);
                break;
            }
        }

        let object = x86_64_object.ok_or_else(|| {
            format!(
                "find x86-64 code in the fat binary `{}` referenced by",
                file_name
            )
        })?;
        let debug_session = object
            .debug_session()
            .map_err(|_| format!("read debug info from `{}` referenced by", file_name))?;

        FileInfo::add(
            &sym_func_addrs,
            file_name,
            debug_session,
            interner,
            func_infos,
        );

        Ok(())
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
        let in_func_name = &captures[2];
        let in_file_name = &captures[3];
        let address = u64::from_str_radix(&captures[4], 16).unwrap();
        let after = &captures[5];

        // If we haven't seen this file yet, parse and record its contents, for
        // this lookup and any future lookups.
        let file_info = match self.file_infos.entry(in_file_name.to_string()) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => match Fixer::build_file_info(in_file_name) {
                Ok(file_info) => v.insert(file_info),
                Err(op) => {
                    // Print an error message and then set up an empty
                    // `FileInfo` for this file, for two reasons.
                    // - If an invalid file is mentioned multiple times in the
                    //   input, an error message will be issued only on the
                    //   first occurrence.
                    // - The line will still receive some transformation, using
                    //   the "no symbols or debug info" case below.
                    eprintln!("fix-stacks error: failed to {} `{}`", op, in_file_name);
                    v.insert(FileInfo::default())
                }
            },
        };

        // If JSON escaping is enabled, we need to escape any new strings we
        // produce. However, strings that came in from the text (i.e.
        // `in_func_name` and `in_file_name`), will already be escaped, so if
        // they become part of the output they shouldn't be escaped.
        if let Some(func_info) = file_info.func_info(address) {
            let raw_func_name = func_info.demangled_name();
            let out_func_name = if let JsonEscaping::Yes = self.json_escaping {
                Fixer::json_escape(&raw_func_name)
            } else {
                raw_func_name
            };

            if let Some(line_info) = func_info.line_info(address) {
                // We have the function name, filename, and line number from
                // the debug info.
                let raw_file_name = file_info.interner.get(line_info.path);
                let out_file_name = if let JsonEscaping::Yes = self.json_escaping {
                    Fixer::json_escape(&raw_file_name)
                } else {
                    raw_file_name.to_string()
                };

                format!(
                    "{}{} ({}:{}){}",
                    before, out_func_name, out_file_name, line_info.line, after
                )
            } else {
                // We have the function name from the debug info, but no file
                // name or line number. Use the file name and address from the
                // original input.
                format!(
                    "{}{} ({} +0x{:x}){}",
                    before, out_func_name, in_file_name, address, after
                )
            }
        } else {
            // We have nothing from the debug info. Use the function name, file
            // name, and address from the original input. The end result is the
            // same as the original line, but with slightly different
            // formatting.
            format!(
                "{}{} ({} +0x{:x}{})",
                before, in_func_name, in_file_name, address, after
            )
        }
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
