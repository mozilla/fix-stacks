// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::*;

#[test]
fn test_linux() {
    // The native debug info within `example-linux` is as follows. (See
    // `tests/README.md` for details on how these lines were generated.)
    //
    //   FUNC 0x1130 size=40 func=main
    //   LINE 0x1130 line=24 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x113f line=25 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x1146 line=26 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x114f line=27 file=/home/njn/moz/fix-stacks/tests/example.c
    //
    //   FUNC 0x1160 size=69 func=f
    //   LINE 0x1160 line=16 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x116c line=17 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x1177 line=18 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x1180 line=19 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x118b line=20 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x1194 line=21 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x119f line=22 file=/home/njn/moz/fix-stacks/tests/example.c
    //
    //   FUNC 0x11b0 size=49 func=g
    //   LINE 0x11b0 line=11 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x11bc line=12 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x11cd line=13 file=/home/njn/moz/fix-stacks/tests/example.c
    //   LINE 0x11db line=14 file=/home/njn/moz/fix-stacks/tests/example.c

    let mut fixer = Fixer::new(JsonMode::No, None);

    // Test various addresses.
    let mut func = |name, addr, linenum| {
        let line = format!("#00: ???[tests/example-linux +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(
            line,
            format!(
                "#00: {} (/home/njn/moz/fix-stacks/tests/example.c:{})",
                name, linenum
            )
        );
    };
    func("main", 0x1130, 24);
    func("main", 0x1131, 24);
    func("main", 0x1132, 24);
    func("main", 0x1137, 24);
    func("main", 0x113a, 24);
    func("main", 0x113d, 24);
    func("main", 0x113e, 24);
    func("main", 0x113f, 25);
    func("main", 0x1146, 26);
    func("main", 0x114e, 26);
    func("main", 0x1157, 27);
    func("f", 0x1160, 16);
    func("f", 0x1180, 19);
    func("g", 0x11bc, 12);
    func("g", 0x11de, 14);

    // Try a new Fixer.
    fixer = Fixer::new(JsonMode::No, None);

    // Test various addresses outside `main`, `f`, and `g`.
    let mut outside = |addr| {
        let line = format!("#00: ???[tests/example-linux +0x{:x}]", addr);
        let line_actual = fixer.fix(line);
        let line_expected = format!("#00: ??? (tests/example-linux + 0x{:x})", addr);
        assert_eq!(line_expected, line_actual);
    };
    outside(0x0); // A very low address.
    outside(0x999); // Well before the start of main.
    outside(0x112f); // One byte before the start of `main`.
    outside(0x1158); // One byte past the end of `main`.
    outside(0xfffffff); // A very high address.
}

#[test]
fn test_windows() {
    // The native debug info within `example-windows.pdb` is as follows. (See
    // `tests/README.md` for details on how these lines were generated.)
    //
    //   FUNC 0x6bc0 size=39 func=main
    //   LINE 0x6bc0 line=24 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6bcc line=25 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6bd4 line=26 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6bde line=27 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //
    //   FUNC 0x6bf0 size=70 func=f
    //   LINE 0x6bf0 line=16 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6bf9 line=17 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6c05 line=18 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6c0f line=19 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6c1b line=20 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6c25 line=21 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6c31 line=22 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //
    //   FUNC 0x6c40 size=38 func=g
    //   LINE 0x6c40 line=11 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6c49 line=12 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6c55 line=13 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //   LINE 0x6c61 line=14 file=c:\Users\njn\moz\fix-stacks\tests\example.c

    // Note: this test uses a forward slash as the directory separator in all
    // input lines, so that it will work on all platforms. (All platforms can
    // handle forward slashes; Windows can also handle backward slashes.) The
    // outputs contains backwards slashes, though, because that is what is used
    // within the debug info.

    let mut fixer = Fixer::new(JsonMode::Yes, None);

    // Test various addresses using `example-windows.exe`, which redirects to
    // `example-windows.pdb`.
    let mut func = |name, addr, linenum| {
        let line = format!("#00: ???[tests/example-windows.exe +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(
            line,
            format!(
                // The extra backslashes here are due to the JSON escaping.
                "#00: {} (c:\\\\Users\\\\njn\\\\moz\\\\fix-stacks\\\\tests\\\\example.c:{})",
                name, linenum
            )
        );
    };
    func("main", 0x6bc0, 24);
    func("main", 0x6bc1, 24);
    func("main", 0x6bc2, 24);
    func("main", 0x6bc7, 24);
    func("main", 0x6bc9, 24);
    func("main", 0x6bca, 24);
    func("main", 0x6bcb, 24);
    func("main", 0x6bcf, 25);
    func("main", 0x6bd6, 26);
    func("main", 0x6bdd, 26);
    func("main", 0x6be6, 27);
    func("f", 0x6bf4, 16);
    func("f", 0x6c0f, 19);
    func("g", 0x6c49, 12);
    func("g", 0x6c63, 14);

    // Try a new Fixer, without JSON mode.
    fixer = Fixer::new(JsonMode::No, None);

    // Test various addresses outside `main`, `f`, and `g`, using
    // `example-windows.pdb` directly.
    let mut outside = |addr| {
        let line = format!("#00: foobar[tests/example-windows.pdb +0x{:x}]", addr);
        let line_actual = fixer.fix(line);
        let line_expected = format!("#00: foobar (tests/example-windows.pdb + 0x{:x})", addr);
        assert_eq!(line_expected, line_actual);
    };
    outside(0x0); // A very low address.
    outside(0x999); // Well before the start of main.
    outside(0x6bbf); // One byte before the start of `main`.
    outside(0x6be7); // One byte past the end of `main`.
    outside(0xfffffff); // A very high address.
}

#[test]
fn test_mac() {
    // The native debug info within `mac-multi` is as follows. (See
    // `tests/README.md` for details on how these lines were generated.)
    //
    //   FUNC 0xd70 size=54 func=main
    //   LINE 0xd70 line=17 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //   LINE 0xd7f line=18 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //   LINE 0xd86 line=19 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //   LINE 0xd8f line=20 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //   LINE 0xd98 line=21 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //   LINE 0xd9d line=22 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //
    //   FUNC 0xdb0 size=31 func=duplicate
    //   LINE 0xdb0 line=10 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //   LINE 0xdb8 line=11 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //   LINE 0xdc9 line=12 file=/Users/njn/moz/fix-stacks/tests/mac-normal.c
    //
    //   FUNC 0xdd0 size=37 func=fat_B
    //   LINE 0xdd0 line=19 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //   LINE 0xddc line=20 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //   LINE 0xdef line=21 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //
    //   FUNC 0xe00 size=34 func=fat_A
    //   LINE 0xe00 line=13 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //   LINE 0xe0b line=14 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //   LINE 0xe14 line=15 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //   LINE 0xe19 line=16 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //
    //   FUNC 0xe30 size=31 func=duplicate
    //   LINE 0xe30 line=9 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //   LINE 0xe38 line=10 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //   LINE 0xe49 line=11 file=/Users/njn/moz/fix-stacks/tests/mac-fat.c
    //
    //   FUNC 0xe50 size=37 func=lib1_B
    //   LINE 0xe50 line=19 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //   LINE 0xe5c line=20 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //   LINE 0xe6f line=21 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //
    //   FUNC 0xe80 size=34 func=lib1_A
    //   LINE 0xe80 line=13 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //   LINE 0xe8b line=14 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //   LINE 0xe94 line=15 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //   LINE 0xe99 line=16 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //
    //   // Note that these ones are wrong, and should start at 0xeb0. This is
    //   // due to the unavoidable archive suffix stripping, see comments in
    //   // `main.rs` for details.
    //   FUNC 0xf30 size=31 func=duplicate
    //   LINE 0xf30 line=9 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //   LINE 0xf38 line=10 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //   LINE 0xf49 line=11 file=/Users/njn/moz/fix-stacks/tests/mac-lib1.c
    //
    //   FUNC 0xed0 size=37 func=lib2_B
    //   LINE 0xed0 line=19 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //   LINE 0xedc line=20 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //   LINE 0xeef line=21 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //
    //   FUNC 0xf00 size=39 func=lib2_A
    //   LINE 0xf00 line=13 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //   LINE 0xf0b line=14 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //   LINE 0xf19 line=15 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //   LINE 0xf1e line=16 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //
    //   FUNC 0xf30 size=31 func=duplicate
    //   LINE 0xf30 line=9 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //   LINE 0xf38 line=10 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c
    //   LINE 0xf49 line=11 file=/Users/njn/moz/fix-stacks/tests/mac-lib2.c

    let mut fixer = Fixer::new(JsonMode::No, None);

    // Test addresses from all the object files that `mac-multi` references.
    let mut func = |name, addr, full_path, locn| {
        let line = format!("#00: ???[tests/mac-multi +0x{:x}]", addr);
        let line_actual = fixer.fix(line);
        let path = if full_path {
            "/Users/njn/moz/fix-stacks/tests/"
        } else {
            "tests/"
        };
        let line_expected = format!("#00: {} ({}{})", name, path, locn);
        assert_eq!(line_expected, line_actual);
    };

    func("main", 0xd70, true, "mac-normal.c:17");
    func("duplicate", 0xdb3, true, "mac-normal.c:10");

    func("fat_B", 0xddc, true, "mac-fat.c:20");
    func("fat_A", 0xe19, true, "mac-fat.c:16");
    func("duplicate", 0xe4e, true, "mac-fat.c:11");

    func("lib1_B", 0xe50, true, "mac-lib1.c:19");
    func("lib1_A", 0xe95, true, "mac-lib1.c:15");
    // This should be `duplicate` in `mac-lib1.c`. It's wrong due to the
    // archive suffix stripping mentioned above.
    func("???", 0xeaa, false, "mac-multi + 0xeaa");

    func("lib2_B", 0xedc, true, "mac-lib2.c:20");
    func("lib2_A", 0xf1e, true, "mac-lib2.c:16");
    // This should be `mac-lib2.c:10`. It's wrong due to the archive suffix
    // stripping mentioned above.
    func("duplicate", 0xf38, true, "mac-lib1.c:10");
}

#[test]
fn test_linux_breakpad() {
    // The breakpad symbols debug info within `bpsyms/example-linux/` is as
    // follows. (See `tests/README.md` for details on how these lines were
    // generated.)
    //
    // FUNC 0x1130 size=40 func=main
    // LINE 0x1130 line=24 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x113f line=25 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x1146 line=26 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x114f line=27 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x1152 line=27 file=/home/njn/moz/fix-stacks/tests/example.c
    //
    // FUNC 0x1160 size=69 func=f
    // LINE 0x1160 line=16 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x116c line=17 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x1170 line=17 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x1177 line=18 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x117b line=18 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x1180 line=19 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x1184 line=19 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x118b line=20 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x118f line=20 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x1194 line=21 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x1198 line=21 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x119f line=22 file=/home/njn/moz/fix-stacks/tests/example.c
    //
    // FUNC 0x11b0 size=49 func=g
    // LINE 0x11b0 line=11 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x11bc line=12 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x11cd line=13 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x11d1 line=13 file=/home/njn/moz/fix-stacks/tests/example.c
    // LINE 0x11db line=14 file=/home/njn/moz/fix-stacks/tests/example.c
    let mut fixer = Fixer::new(
        JsonMode::No,
        Some(BreakpadInfo {
            syms_dir: "tests/bpsyms".to_string(),
        }),
    );

    // Test various addresses.
    let mut func = |name, addr, linenum| {
        let line = format!("#00: ???[tests/example-linux +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(
            line,
            format!(
                "#00: {} [/home/njn/moz/fix-stacks/tests/example.c:{}]",
                name, linenum
            )
        );
    };
    func("main", 0x1130, 24);
    func("main", 0x113f, 25);
    func("main", 0x1146, 26);
    func("main", 0x1157, 27);
    func("f", 0x1160, 16);
    func("f", 0x1180, 19);
    func("g", 0x11bc, 12);
    func("g", 0x11de, 14);

    // Test various addresses outside `main`, `f`, and `g`.
    let mut outside = |addr| {
        let line = format!("#00: ???[tests/example-linux +0x{:x}]", addr);
        let line_actual = fixer.fix(line);
        let line_expected = format!("#00: ??? [tests/example-linux + 0x{:x}]", addr);
        assert_eq!(line_expected, line_actual);
    };
    outside(0x0); // A very low address.
    outside(0xfffffff); // A very high address.
}

#[test]
fn test_linux_breakpad_fallback() {
    // The breakpad symbols debug info within `bpsyms/` is missing in this
    // test. This verifies that we fall back to using native debug information
    // correctly in this scenario.
    //
    // The native debug info within `example-linux-fallback` is as follows. (See
    // `tests/README.md` for details on how these lines were generated.)
    //
    // FUNC 11f8 size=67 func=main
    // LINE 0x11f8 line=24 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x120f line=25 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x1216 line=26 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x1222 line=27 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x1225 line=28 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // FUNC 11a4 size=84 func=f
    // LINE 0x11a4 line=16 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x11b0 line=17 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x11bf line=18 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x11cb line=19 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x11da line=20 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x11e6 line=21 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x11f5 line=22 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // FUNC 1175 size=47 func=g
    // LINE 0x1175 line=11 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x1181 line=12 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x1192 line=13 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    // LINE 0x11a1 line=14 file=/home/gsvelto/projects/fix-stacks/tests/example.c
    let mut fixer = Fixer::new(
        JsonMode::No,
        Some(BreakpadInfo {
            syms_dir: "tests/bpsyms".to_string(),
        }),
    );

    // Test various addresses.
    let mut func = |name, addr, linenum| {
        let line = format!("#00: ???[tests/example-linux-fallback +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(
            line,
            format!(
                "#00: {} [/home/gsvelto/projects/fix-stacks/tests/example.c:{}]",
                name, linenum
            )
        );
    };
    func("main", 0x11f8, 24);
    func("main", 0x120f, 25);
    func("main", 0x1216, 26);
    func("main", 0x1222, 27);
    func("f", 0x11a4, 16);
    func("f", 0x11cb, 19);
    func("g", 0x1181, 12);
    func("g", 0x11a1, 14);

    // Test various addresses outside `main`, `f`, and `g`.
    let mut outside = |addr| {
        let line = format!("#00: ???[tests/example-linux-fallback +0x{:x}]", addr);
        let line_actual = fixer.fix(line);
        let line_expected = format!("#00: ??? [tests/example-linux-fallback + 0x{:x}]", addr);
        assert_eq!(line_expected, line_actual);
    };
    outside(0x0); // A very low address.
    outside(0xfffffff); // A very high address.
}

#[test]
fn test_windows_breakpad() {
    // The breakpad symbols debug info within `bpsyms/example-windows.pdb/` is
    // as follows. (See `tests/README.md` for details on how these lines were
    // generated.)
    //
    // FUNC 0x6bc0 size=39 func=main
    // LINE 0x6bc0 line=24 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6bcc line=25 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6bd4 line=26 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6bde line=27 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //
    // FUNC 0x6bf0 size=70 func=f
    // LINE 0x6bf0 line=16 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6bf9 line=17 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6c05 line=18 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6c0f line=19 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6c1b line=20 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6c25 line=21 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6c31 line=22 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    //
    // FUNC 0x6c40 size=38 func=g
    // LINE 0x6c40 line=11 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6c49 line=12 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6c55 line=13 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    // LINE 0x6c61 line=14 file=c:\Users\njn\moz\fix-stacks\tests\example.c
    let mut fixer = Fixer::new(
        JsonMode::No,
        Some(BreakpadInfo {
            syms_dir: "tests/bpsyms".to_string(),
        }),
    );

    // Test various addresses.
    let mut func = |name, addr, linenum| {
        let line = format!("#00: ???[tests/example-windows.exe +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(
            line,
            format!(
                "#00: {} [c:\\Users\\njn\\moz\\fix-stacks\\tests\\example.c:{}]",
                name, linenum
            )
        );
    };
    func("main()", 0x6bc0, 24);
    func("main()", 0x6bce, 25);
    func("main()", 0x6bd4, 26);
    func("main()", 0x6bdf, 27);
    func("f(int*)", 0x6bf0, 16);
    func("f(int*)", 0x6c0f, 19);
    func("g(int*)", 0x6c49, 12);
    func("g(int*)", 0x6c61, 14);

    // Test various addresses outside `main`, `f`, and `g`.
    let mut outside = |addr| {
        let line = format!("#00: ???[tests/example-windows.exe +0x{:x}]", addr);
        let line_actual = fixer.fix(line);
        let line_expected = format!("#00: ??? [tests/example-windows.exe + 0x{:x}]", addr);
        assert_eq!(line_expected, line_actual);
    };
    outside(0x0); // A very low address.
    outside(0xfffffff); // A very high address.
}

#[test]
fn test_regex() {
    let mut fixer = Fixer::new(JsonMode::No, None);

    // Test various different unchanged line forms, that don't match the regex.
    let mut unchanged = |line: &str| {
        let line2 = fixer.fix(line.to_string());
        assert_eq!(line, line2);
    };
    unchanged("");
    unchanged("1234 ABCD");
    unchanged("00: ???[tests/example-linux +0x1130]"); // Missing the leading '#'.
    unchanged("#00: ???[tests/example-linux 0x1130]"); // Missing the '+' before the address.
    unchanged("#00: ???[tests/example-linux +1130]"); // Missing the '0x`.
    unchanged("#00: ???(tests/example-linux +0x1130)"); // Wrong parentheses.

    // Test various different changed line forms that do match the regex.
    let mut changed = |line1: &str, line2_expected| {
        let line2_actual = fixer.fix(line1.to_string());
        assert_eq!(line2_expected, line2_actual);
    };
    changed(
        "#01: foobar[tests/example-linux +0x1130]",
        "#01: main (/home/njn/moz/fix-stacks/tests/example.c:24)",
    );
    changed(
        "PREFIX#9999: ???[tests/example-linux +0x1130]SUFFIX",
        "PREFIX#9999: main (/home/njn/moz/fix-stacks/tests/example.c:24)SUFFIX",
    );
    changed(
        "#01: ???[tests/../src/../tests/example-linux +0x1130]",
        "#01: main (/home/njn/moz/fix-stacks/tests/example.c:24)",
    );
}

#[test]
fn test_files() {
    let mut fixer = Fixer::new(JsonMode::Yes, None);

    // Test various different file errors. An error message is also printed to
    // stderr for each one, but we don't test for that.
    let mut file_error = |line1: &str, line2_expected| {
        let line2_actual = fixer.fix(line1.to_string());
        assert_eq!(line2_expected, line2_actual);
    };
    // No such file.
    file_error(
        "#00: ???[tests/no-such-file +0x0]",
        "#00: ??? (tests/no-such-file + 0x0)",
    );
    // No such file, with backslashes (which tests JSON escaping).
    file_error(
        "#00: ???[tests\\no-such-dir\\\\no-such-file +0x0]",
        "#00: ??? (tests\\no-such-dir\\\\no-such-file + 0x0)",
    );
    // File exists, but has the wrong format.
    file_error("#00: ???[src/main.rs +0x0]", "#00: ??? (src/main.rs + 0x0)");
}
