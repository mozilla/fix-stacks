// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use anyhow::{bail, Context, Result};
use fxhash::{FxHashMap, FxHashSet};
use goblin::{archive, mach};
use regex::Regex;
use std::collections::hash_map::Entry;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::str;
use symbolic_common::{Arch, Language, Name, NameMangling};
use symbolic_debuginfo::{Archive, FileFormat, Function, Object, ObjectDebugSession};
use symbolic_demangle::{Demangle, DemangleOptions};

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

enum JsonMode {
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
        let options = DemangleOptions::complete();
        Name::new(
            self.mangled_name.as_str(),
            NameMangling::Mangled,
            Language::Unknown,
        )
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

/// Info provided via the `-b` flag.
struct BreakpadInfo {
    syms_dir: String,
}

struct LocalFileInfo {
    local_dir: String,
}

trait CpuArch {
    fn cpuarch(&self) -> Arch;
}

impl CpuArch for mach::header::Header {
    fn cpuarch(&self) -> Arch {
        // Copied from symbolic_debug::macho::MachObject::arch.
        use mach::constants::cputype;

        match (self.cputype(), self.cpusubtype()) {
            (cputype::CPU_TYPE_I386, cputype::CPU_SUBTYPE_I386_ALL) => Arch::X86,
            (cputype::CPU_TYPE_I386, _) => Arch::X86Unknown,
            (cputype::CPU_TYPE_X86_64, cputype::CPU_SUBTYPE_X86_64_ALL) => Arch::Amd64,
            (cputype::CPU_TYPE_X86_64, cputype::CPU_SUBTYPE_X86_64_H) => Arch::Amd64h,
            (cputype::CPU_TYPE_X86_64, _) => Arch::Amd64Unknown,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_ALL) => Arch::Arm64,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_V8) => Arch::Arm64V8,
            (cputype::CPU_TYPE_ARM64, cputype::CPU_SUBTYPE_ARM64_E) => Arch::Arm64e,
            (cputype::CPU_TYPE_ARM64, _) => Arch::Arm64Unknown,
            (cputype::CPU_TYPE_ARM64_32, cputype::CPU_SUBTYPE_ARM64_32_ALL) => Arch::Arm64_32,
            (cputype::CPU_TYPE_ARM64_32, cputype::CPU_SUBTYPE_ARM64_32_V8) => Arch::Arm64_32V8,
            (cputype::CPU_TYPE_ARM64_32, _) => Arch::Arm64_32Unknown,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_ALL) => Arch::Arm,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V5TEJ) => Arch::ArmV5,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V6) => Arch::ArmV6,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V6M) => Arch::ArmV6m,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7) => Arch::ArmV7,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7F) => Arch::ArmV7f,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7S) => Arch::ArmV7s,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7K) => Arch::ArmV7k,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7M) => Arch::ArmV7m,
            (cputype::CPU_TYPE_ARM, cputype::CPU_SUBTYPE_ARM_V7EM) => Arch::ArmV7em,
            (cputype::CPU_TYPE_ARM, _) => Arch::ArmUnknown,
            (cputype::CPU_TYPE_POWERPC, cputype::CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc,
            (cputype::CPU_TYPE_POWERPC64, cputype::CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc64,
            (_, _) => Arch::Unknown,
        }
    }
}

/// The top level structure that does the work.
struct Fixer {
    re: Regex,
    file_infos: FxHashMap<String, FileInfo>,
    json_mode: JsonMode,
    bp_info: Option<BreakpadInfo>,
    local_info: Option<LocalFileInfo>,
    lb: char,
    rb: char,
}

/// Records address of functions from a symbol table.
type SymFuncAddrs = FxHashMap<String, u64>;

impl Fixer {
    fn new(
        json_mode: JsonMode,
        bp_info: Option<BreakpadInfo>,
        local_info: Option<LocalFileInfo>,
    ) -> Fixer {
        // We use parentheses with native debug info, and square brackets with
        // Breakpad symbols.
        let (lb, rb) = if bp_info.is_none() {
            ('(', ')')
        } else {
            ('[', ']')
        };
        Fixer {
            // Matches lines produced by MozFormatCodeAddress().
            re: Regex::new(r"^(.*#\d+: )(.+)\[(.+) \+0x([0-9A-Fa-f]+)\](.*)$").unwrap(),
            file_infos: FxHashMap::default(),
            json_mode,
            bp_info,
            local_info,
            lb,
            rb,
        }
    }

    /// Add JSON escapes to a fragment of text.
    fn json_escape(string: &str) -> String {
        // Do the escaping.
        let escaped = serde_json::to_string(string).unwrap();

        // Strip the quotes.
        escaped[1..escaped.len() - 1].to_string()
    }

    /// Remove JSON escapes from a fragment of text.
    fn json_unescape(string: &str) -> String {
        // Add quotes.
        let quoted = format!("\"{}\"", string);

        // Do the unescaping, which also removes the quotes.
        let value = serde_json::from_str(&quoted).unwrap();
        if let serde_json::Value::String(unescaped) = value {
            unescaped
        } else {
            panic!()
        }
    }

    /// Read the data from `file_name` and construct a `FileInfo` that we can
    /// subsequently query. Return a description of the failing operation on
    /// error.
    fn build_file_info(bin_file: &str, bp_info: &Option<BreakpadInfo>) -> Result<FileInfo> {
        // If we're using Breakpad symbols, we don't consult `bin_file`.
        if let Some(bp_info) = bp_info {
            if let Ok(res) = Fixer::build_file_info_breakpad(bin_file, bp_info) {
                return Ok(res);
            }
        }

        // Otherwise, we read `bin_file`.
        let data = fs::read(bin_file).context("read")?;
        let file_format = Archive::peek(&data);
        match file_format {
            FileFormat::Elf => Fixer::build_file_info_direct(&data),
            FileFormat::Pe => Fixer::build_file_info_pe(&data),
            FileFormat::Pdb => Fixer::build_file_info_direct(&data),
            FileFormat::MachO => Fixer::build_file_info_macho(&data),
            _ => bail!("parse {} format file", file_format),
        }
    }

    fn build_file_info_breakpad(
        bin_file: &str,
        BreakpadInfo { syms_dir }: &BreakpadInfo,
    ) -> Result<FileInfo> {
        // We must find the `.sym` file for this `bin_file`, as produced by the
        // Firefox build system, which is in the symbols directory under
        // `<db_seg>/<uuid_seg>/<sym_seg>`.
        //
        // A running example:
        // - Unix and windows: `syms_dir` is `syms/`
        // - Unix: `bin_file` is `bin/libxul.so`
        // - Unix: symbols are in `syms/libxul.so/<uuid>/libxul.so.sym`
        // - Windows: `bin_file` is bin/xul.dll`
        // - Windows: symbols are in `syms/xul.pdb/<uuid>/xul.sym`
        let bin_file = Path::new(bin_file);

        // - Unix: `bin_base` is `libxul.so`
        // - Windows: `bin_base` is `xul`
        let mut bin_base = bin_file
            .file_name()
            .context("read breakpad symbols for")?
            .to_str()
            .unwrap()
            .to_string();
        let is_win = bin_base.ends_with(".dll") || bin_base.ends_with(".exe");
        if is_win {
            bin_base.truncate(bin_base.len() - 4);
        }

        // - Unix: `db_seg` is `libxul.so`
        // - Windows: `db_seg` is `xul.pdb`
        let mut db_seg = bin_base.clone();
        if is_win {
            db_seg.push_str(".pdb");
        }

        // - Unix: `db_dir` is `syms/libxul.so/`
        // - Windows: `db_dir` is `syms/xul.pdb/`
        let mut db_dir = PathBuf::new();
        db_dir.push(syms_dir);
        db_dir.push(&db_seg);

        // - Unix: `uuid_dir` is `syms/libxul.so/<uuid>/`
        // - Windows: `uuid_dir` is `syms/xul.pdb/<uuid>/`
        let uuid_dir = {
            let data = fs::read(bin_file).context("read")?;
            let object = Object::parse(&data).context("parse")?;
            let uuid_seg = object.debug_id().breakpad().to_string();
            let mut uuid_dir = db_dir;
            uuid_dir.push(uuid_seg);
            uuid_dir
        };

        // - Unix: `sym_seg` is `libxul.so.sym`
        // - Windows: `sym_seg` is `xul.sym`
        let mut sym_seg = bin_base;
        sym_seg.push_str(".sym");

        // - Unix: `sym_file` is `syms/libxul.so/<uuid>/libxul.so.sym`.
        // - Windows: `sym_file` is `syms/xul.pdb/<uuid>/xul.sym`.
        let mut sym_file = uuid_dir;
        sym_file.push(&sym_seg);

        let data = fs::read(&sym_file)
            .context(
                "note: this is expected and harmless for system libraries on debug automation runs",
            )
            .with_context(|| format!("read symbols file `{}` for", sym_file.display()))?;
        Fixer::build_file_info_direct(&data)
    }

    // "Direct" means that the debug info is within `data`, as opposed to being
    // in another file that `data` refers to.
    fn build_file_info_direct(data: &[u8]) -> Result<FileInfo> {
        let object = Object::parse(data).context("parse")?;
        let debug_session = object.debug_session().context("read debug info from")?;
        Ok(FileInfo::new(debug_session))
    }

    fn build_file_info_pe(data: &[u8]) -> Result<FileInfo> {
        // For PEs we get the debug info from a PDB file.
        let pe_object = Object::parse(data).context("parse")?;
        let pe = match pe_object {
            Object::Pe(pe) => pe,
            _ => unreachable!(),
        };
        let pdb_file_name = pe.debug_file_name().context("find debug info file for")?;
        let data = fs::read(pdb_file_name.to_string())
            .context("note: this is expected and harmless for all PDB files on opt automation runs")
            .with_context(|| format!("read debug info file `{}` for", pdb_file_name))?;
        Fixer::build_file_info_direct(&data)
    }

    fn build_file_info_macho(data: &[u8]) -> Result<FileInfo> {
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

        let macho = Fixer::macho(data)?;
        let arch = macho.header.cpuarch();
        let sym_func_addrs = Fixer::sym_func_addrs(&macho)?;

        // Iterate again through the symbol table, reading every object file
        // that is referenced, and adjusting the addresses in those files using
        // the function addresses obtained above.
        let mut seen_archives = FxHashSet::default();
        let mut func_infos = vec![];
        let mut interner = Interner::default();
        for sym in macho.symbols() {
            let (oso_name, nlist) = sym.context("read symbol table from")?;
            if nlist.is_stab() && nlist.n_type == mach::symbols::N_OSO {
                if let Some(ar_file_name) = Fixer::is_within_archive(oso_name) {
                    // It's an archive entry, e.g. "libgkrust.a(foo.o)". Read
                    // every entry in archive, if we haven't already done so.
                    if seen_archives.insert(ar_file_name) {
                        let ar_data = fs::read(ar_file_name)
                            .with_context(|| format!("read ar `{}` referenced by", ar_file_name))?;
                        let ar = archive::Archive::parse(&ar_data).with_context(|| {
                            format!("parse ar `{}` referenced by", ar_file_name)
                        })?;

                        for (name, _, _) in ar.summarize() {
                            let data = ar.extract(name, &ar_data).with_context(|| {
                                format!("read an entry in ar `{}` referenced by", ar_file_name)
                            })?;
                            Fixer::do_macho_oso(
                                &sym_func_addrs,
                                ar_file_name,
                                data,
                                &mut interner,
                                &mut func_infos,
                                arch,
                            )?;
                        }
                    }
                } else {
                    // It's a normal object file. Read it.
                    let note = "note: this is expected and harmless for all Mac object files on opt automation runs";
                    let data = fs::read(oso_name).context(note).with_context(|| {
                        format!("read object file `{}` referenced by", oso_name)
                    })?;
                    Fixer::do_macho_oso(
                        &sym_func_addrs,
                        oso_name,
                        &data,
                        &mut interner,
                        &mut func_infos,
                        arch,
                    )?;
                }
            }
        }

        Ok(FileInfo::finish(interner, func_infos))
    }

    fn macho(data: &[u8]) -> Result<mach::MachO> {
        let mach = mach::Mach::parse(data).context("parse (with goblin)")?;
        match mach {
            mach::Mach::Binary(macho) => Ok(macho),
            mach::Mach::Fat(multi_arch) => {
                // There is no way to know which side of the fat binary is refered by a
                // stack frame line, so take our best guess, which is whichever target
                // fix-stacks itself was compiled for.
                const CPU_TYPE: mach::cputype::CpuType = if cfg!(target_arch = "x86_64") {
                    mach::constants::cputype::CPU_TYPE_X86_64
                } else if cfg!(target_arch = "x86") {
                    mach::constants::cputype::CPU_TYPE_X86
                } else if cfg!(target_arch = "aarch64") {
                    mach::constants::cputype::CPU_TYPE_ARM64
                } else {
                    // The fallback is meant to match no CPU type.
                    mach::constants::cputype::CPU_TYPE_ANY
                };
                let macho = multi_arch.into_iter().find(|macho| {
                    if let Ok(macho) = macho {
                        if macho.header.cputype() == CPU_TYPE {
                            return true;
                        }
                    }
                    false
                });

                // This chaining is necessary because `MachOIterator::Item` is
                // not `MachO` but `Result<MachO>`. We don't distinguish
                // between the "couldn't find the $target_arch code" case and the
                // "found it, but it had an error" case.
                let msg = if cfg!(target_arch = "x86_64") {
                    "find x86_64 code in the fat binary"
                } else if cfg!(target_arch = "x86") {
                    "find x86 code in the fat binary"
                } else if cfg!(target_arch = "aarch64") {
                    "find arm64 code in the fat binary"
                } else {
                    "decide what code to use in the fat binary"
                };
                macho.context(msg)?.context(msg)
            }
        }
    }

    /// Iterate through the symbol table, getting the address of every function
    /// in the file.
    fn sym_func_addrs(macho: &mach::MachO) -> Result<SymFuncAddrs> {
        let object_load_address = Fixer::object_load_address(macho);
        let mut sym_func_addrs = FxHashMap::default();
        let mut curr_oso_name = String::new();
        for sym in macho.symbols() {
            let (name, nlist) = sym.context("read symbol table from")?;
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

    /// Strip any annoying Firefox Breakpad junk from this filename. E.g.
    /// `hg:hg.mozilla.org/integration/autoland:caps/BasePrincipal.cpp:04c31e994f29e72dd81a7340100d12f67e48a5b4`
    /// becomes `caps/BasePrincipal.cpp`.
    ///
    /// It's not perfect, e.g. it will fail if the filename contains a colon.
    /// But that should happen almost never, and if it does the junk won't be
    /// stripped, which is still a reasonable outcome.
    fn strip_firefox_breakpad_junk(file_name: &str) -> Option<&str> {
        // Split on the colons.
        let mut iter = file_name.split(':');

        // Is the first element "hg"?
        let s1 = iter.next()?;
        if s1 != "hg" {
            return None;
        }

        // Does the second element start with "hg.mozilla.org"?
        let s2 = iter.next()?;
        if !s2.starts_with("hg.mozilla.org") {
            return None;
        }

        // The third element is the one we want.
        let s3 = iter.next()?;

        // Is the fourth element a hex id of length 40?
        let s4 = iter.next()?;
        if s4.len() != 40 || !s4.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }

        // Is there no fifth element?
        if iter.next().is_some() {
            return None;
        }

        // It's a match. Return the interesting part.
        Some(s3)
    }

    /// Read the debug info from a file referenced by an OSO entry in a Macho-O
    /// symbol table.
    fn do_macho_oso(
        sym_func_addrs: &SymFuncAddrs,
        file_name: &str,
        data: &[u8],
        interner: &mut Interner,
        func_infos: &mut Vec<FuncInfo>,
        arch: Arch,
    ) -> Result<()> {
        // Although we use `goblin` to iterate through the symbol
        // table, we use `symbolic` to read the debug info from the
        // object/archive, because it's easier to use.
        let archive =
            Archive::parse(data).with_context(|| format!("parse `{}` referenced by", file_name))?;

        // Get the object of the wanted arch from the archive, which might be a fat binary.
        let mut the_object = None;
        for object in archive.objects() {
            let object = object.with_context(|| {
                format!("parse fat binary entry in `{}` referenced by", file_name)
            })?;
            if object.arch() == arch {
                the_object = Some(object);
                break;
            }
        }

        let object = the_object.with_context(|| {
            format!(
                "find {} code in the fat binary `{}` referenced by",
                arch, file_name
            )
        })?;
        let debug_session = object
            .debug_session()
            .with_context(|| format!("read debug info from `{}` referenced by", file_name))?;

        FileInfo::add(
            sym_func_addrs,
            file_name,
            debug_session,
            interner,
            func_infos,
        );

        Ok(())
    }

    /// Remap the path with local options' path.
    #[inline]
    fn remap(&self, in_file_name: &str) -> Option<String> {
        if let Some(local_info) = &self.local_info {
            if let Some(file_name) = Path::new(in_file_name).file_name() {
                if let Some(new_path) = Path::new(&local_info.local_dir).join(file_name).to_str() {
                    if fs::metadata(new_path).is_ok() {
                        return Some(new_path.to_string());
                    }
                }
            }
        }
        None
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

        // In JSON mode, unescape the function name before using it for
        // lookups, error messages, etc.
        let raw_in_file_name = if let JsonMode::Yes = self.json_mode {
            Fixer::json_unescape(in_file_name)
        } else if fs::metadata(in_file_name).is_ok() {
            in_file_name.to_string()
        } else if let Some(new_path) = self.remap(in_file_name) {
            new_path
        } else {
            // File is not found, but use original path.
            in_file_name.to_string()
        };

        // If we haven't seen this file yet, parse and record its contents, for
        // this lookup and any future lookups.
        let file_info = match self.file_infos.entry(raw_in_file_name.to_string()) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                match Fixer::build_file_info(&raw_in_file_name, &self.bp_info) {
                    Ok(file_info) => v.insert(file_info),
                    Err(err) => {
                        // Print an error message and then set up an empty
                        // `FileInfo` for this file, for two reasons.
                        // - If an invalid file is mentioned multiple times in the
                        //   input, an error message will be issued only on the
                        //   first occurrence.
                        // - The line will still receive some transformation, using
                        //   the "no symbols or debug info" case below.
                        eprintln!(
                            "fix-stacks: error: failed to {} `{}`",
                            err, raw_in_file_name
                        );
                        err.chain()
                            .skip(1)
                            .for_each(|cause| eprintln!("fix-stacks: {}", cause));

                        v.insert(FileInfo::default())
                    }
                }
            }
        };

        // In JSON mode, we need to escape any new strings we produce. However,
        // strings from the input (i.e. `in_func_name` and `in_file_name`),
        // will already be escaped, so if they are used in the output they
        // shouldn't be re-escaped.
        if let Some(func_info) = file_info.func_info(address) {
            let raw_out_func_name = func_info.demangled_name();
            let out_func_name = if let JsonMode::Yes = self.json_mode {
                Fixer::json_escape(&raw_out_func_name)
            } else {
                raw_out_func_name
            };

            if let Some(line_info) = func_info.line_info(address) {
                // We have the function name, filename, and line number from
                // the debug info.
                let raw_out_file_name = file_info.interner.get(line_info.path);
                let out_file_name_str;
                let mut out_file_name = if let JsonMode::Yes = self.json_mode {
                    out_file_name_str = Fixer::json_escape(raw_out_file_name);
                    &out_file_name_str
                } else {
                    raw_out_file_name
                };

                // Maybe strip some junk from Breakpad file names.
                if self.bp_info.is_some() {
                    if let Some(stripped) = Fixer::strip_firefox_breakpad_junk(out_file_name) {
                        out_file_name = stripped
                    }
                };

                format!(
                    "{}{} {}{}:{}{}{}",
                    before, out_func_name, self.lb, out_file_name, line_info.line, self.rb, after
                )
            } else {
                // We have the function name from the debug info, but no file
                // name or line number. Use the file name and address from the
                // original input.
                format!(
                    "{}{} {}{} + 0x{:x}{}{}",
                    before, out_func_name, self.lb, in_file_name, address, self.rb, after
                )
            }
        } else {
            // We have nothing from the debug info. Use the function name, file
            // name, and address from the original input. The end result is the
            // same as the original line, but with slightly different
            // formatting.
            format!(
                "{}{} {}{} + 0x{:x}{}{}",
                before, in_func_name, self.lb, in_file_name, address, self.rb, after
            )
        }
    }
}

#[rustfmt::skip]
const USAGE_MSG: &str =
r##"usage: fix-stacks [options] < input > output

Post-process the stack frames produced by MozFormatCodeAddress().

options:
  -h, --help              Show this message and exit
  -j, --json              Treat input and output as JSON fragments
  -b, --breakpad DIR      Use breakpad symbols in directory DIR
  -l, --local DIR         Remap binary with same file name in DIR if the file
                          is not found
"##;

fn main_inner() -> io::Result<()> {
    // Process command line arguments. The arguments are simple enough for now
    // that using an external crate doesn't seem worthwhile.
    let mut json_mode = JsonMode::No;
    let mut bp_info = None;
    let mut local_info = None;

    let err = |msg| Err(io::Error::new(io::ErrorKind::Other, msg));

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-h" || arg == "--help" {
            println!("{}", USAGE_MSG);
            return Ok(());
        } else if arg == "-j" || arg == "--json" {
            json_mode = JsonMode::Yes;
        } else if arg == "-b" || arg == "--breakpad" {
            match args.next() {
                Some(arg2) => {
                    bp_info = Some(BreakpadInfo {
                        syms_dir: arg2.to_string(),
                    });
                }
                _ => {
                    return err(format!("missing argument to option `{}`.", arg));
                }
            }
        } else if arg == "-l" || arg == "--local" {
            match args.next() {
                Some(arg2) => {
                    local_info = Some(LocalFileInfo {
                        local_dir: arg2.to_string(),
                    });
                }
                _ => {
                    return err(format!("missing argument to option `{}`.", arg));
                }
            }
        } else {
            let msg = format!(
                "bad argument `{}`. Run `fix-stacks -h` for more information.",
                arg
            );
            return err(msg);
        }
    }

    let reader = io::BufReader::new(io::stdin());

    let mut fixer = Fixer::new(json_mode, bp_info, local_info);
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
