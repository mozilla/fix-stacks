// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use regex::Regex;
use std::{
    collections::{HashMap, hash_map::Entry},
    env,
    io::{self, BufRead, Write},
    path::Path,
};
use tokio::{fs, runtime::LocalRuntime};
use wholesym::{LookupAddress, SymbolManager, SymbolManagerConfig, SymbolMap};

#[cfg(test)]
mod tests;

enum JsonMode {
    No,
    Yes,
}

/// Info provided via the `-b` flag.
struct BreakpadInfo {
    syms_dir: String,
}

struct LocalFileInfo {
    local_dir: String,
}

/// The top level structure that does the work.
struct Fixer {
    re: Regex,
    symbol_manager: SymbolManager,
    symbol_maps: HashMap<String, Option<SymbolMap>>,
    json_mode: JsonMode,
    local_info: Option<LocalFileInfo>,
    lb: char,
    rb: char,
}

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
        let mut config = SymbolManagerConfig::new();
        if let Some(bp) = bp_info {
            config = config.breakpad_symbols_dir(bp.syms_dir);
        }
        let symbol_manager = SymbolManager::with_config(config);
        Fixer {
            // Matches lines produced by MozFormatCodeAddress().
            re: Regex::new(r"^(.*#\d+: )(.+)\[(.+) \+0x([0-9A-Fa-f]+)\](.*)$").unwrap(),
            symbol_manager,
            symbol_maps: Default::default(),
            json_mode,
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

    /// Remap the path with local options' path.
    #[inline]
    async fn remap(&self, in_file_name: &str) -> Option<String> {
        if let Some(local_info) = &self.local_info
            && let Some(file_name) = Path::new(in_file_name).file_name()
            && let Some(new_path) = Path::new(&local_info.local_dir).join(file_name).to_str()
            && fs::metadata(new_path).await.is_ok()
        {
            return Some(new_path.to_string());
        }
        None
    }

    /// Fix stack frames within `line` as necessary. Prints any errors to stderr.
    async fn fix(&mut self, line: String) -> String {
        // Apply the regexp.
        let Some(captures) = self.re.captures(&line) else {
            return line;
        };

        let before = &captures[1];
        let in_func_name = &captures[2];
        let in_file_name = &captures[3];
        let Ok(address) = u32::from_str_radix(&captures[4], 16) else {
            eprintln!(
                "fix-stacks: error: address {} out of u32 range",
                &captures[4]
            );
            return line;
        };
        let after = &captures[5];

        match self.try_fix(in_file_name, address).await {
            // We have nothing from the debug info. Use the function name, file
            // name, and address from the original input. The end result is the
            // same as the original line, but with slightly different
            // formatting.
            None => format!(
                "{}{} {}{} + 0x{:x}{}{}",
                before, in_func_name, self.lb, in_file_name, address, self.rb, after
            ),

            // We have the function name from the debug info, but no file
            // name or line number. Use the file name and address from the
            // original input.
            Some((out_func_name, None)) => format!(
                "{}{} {}{} + 0x{:x}{}{}",
                before, out_func_name, self.lb, in_file_name, address, self.rb, after
            ),
            Some((out_func_name, Some((out_file_name, line_number)))) => format!(
                "{}{} {}{}:{}{}{}",
                before, out_func_name, self.lb, out_file_name, line_number, self.rb, after
            ),
        }
    }

    /// Fix stack frames within `line` as necessary. Prints any errors to stderr.
    #[inline]
    async fn try_fix(
        &mut self,
        in_file_name: &str,
        address: u32,
    ) -> Option<(String, Option<(String, u32)>)> {
        // In JSON mode, unescape the function name before using it for
        // lookups, error messages, etc.
        let raw_in_file_name = if let JsonMode::Yes = self.json_mode {
            Fixer::json_unescape(in_file_name)
        } else if fs::metadata(in_file_name).await.is_ok() {
            in_file_name.to_string()
        } else if let Some(new_path) = self.remap(in_file_name).await {
            new_path
        } else {
            // File is not found, but use original path.
            in_file_name.to_string()
        };

        let map = match self.symbol_maps.entry(raw_in_file_name.clone()) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => v.insert(
                self.symbol_manager
                    .load_symbol_map_for_binary_at_path(
                        raw_in_file_name.as_str().as_ref(),
                        Some(wholesym::MultiArchDisambiguator::BestMatchForNative),
                    )
                    .await
                    .inspect_err(|e| eprintln!("fix-stacks: error: {e} `{raw_in_file_name}`"))
                    .ok(),
            ),
        }
        .as_ref()?;

        // In JSON mode, we need to escape any new strings we produce. However, strings from the
        // input (i.e. `in_func_name` and `in_file_name`), will already be escaped, so if they are
        // used in the output they shouldn't be re-escaped.
        if let Some(address_info) = map.lookup(LookupAddress::Relative(address)).await {
            let raw_out_func_name = address_info.symbol.name;

            let out_func_name = if let JsonMode::Yes = self.json_mode {
                Fixer::json_escape(&raw_out_func_name)
            } else {
                raw_out_func_name
            };

            if let Some((file_path, line_number)) = address_info
                .frames
                .and_then(|mut fs| fs.pop())
                .and_then(|frame| frame.file_path.zip(frame.line_number))
            {
                // We have the function name, filename, and line number from
                // the debug info.
                let raw_out_file_name = file_path.display_path();
                let mut out_file_name = if let JsonMode::Yes = self.json_mode {
                    Fixer::json_escape(&raw_out_file_name)
                } else {
                    raw_out_file_name
                };

                // Maybe strip some junk from Breakpad file names.
                if let Some(stripped) = Fixer::strip_firefox_breakpad_junk(&out_file_name) {
                    out_file_name = stripped.into();
                }

                Some((out_func_name, Some((out_file_name, line_number))))
            } else {
                Some((out_func_name, None))
            }
        } else {
            None
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

    let err = |msg| Err(io::Error::other(msg));

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

    let rt = LocalRuntime::new().unwrap();
    let mut fixer = Fixer::new(json_mode, bp_info, local_info);
    for line in reader.lines() {
        writeln!(io::stdout(), "{}", rt.block_on(fixer.fix(line.unwrap())))?;
    }

    Ok(())
}

fn main() {
    // Ignore broken pipes, e.g. when piping output through `head -10`.
    if let Err(err) = main_inner()
        && err.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("fix-stacks: {}", err);
        std::process::exit(1);
    }
}
