// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::*;

#[test]
fn test_linux() {
    // The debug info within `example-linux` is as follows. (See
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

    let mut fixer = Fixer::new(JsonEscaping::No);

    // Test various addresses within `main`.
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
    fixer = Fixer::new(JsonEscaping::No);

    // Test various addresses outside `main`, `f`, and `g`.
    let mut outside = |addr| {
        let line = format!("#00: ???[tests/example-linux +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(format!("#00: ??? (tests/example-linux)"), line);
    };
    outside(0x0); // A very low address.
    outside(0x999); // Well before the start of main.
    outside(0x112f); // One byte before the start of `main`.
    outside(0x1158); // One byte past the end of `main`.
    outside(0xfffffff); // A very high address.
}

#[test]
fn test_windows() {
    // The debug info within `example-windows.pdb` is as follows. (See
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

    let mut fixer = Fixer::new(JsonEscaping::Yes);

    // Test various addresses within `main` using `example-windows`, which
    // redirects to `example-windows.pdb`.
    let mut func = |name, addr, linenum| {
        let line = format!("#00: ???[tests/example-windows +0x{:x}]", addr);
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

    // Try a new Fixer, without JSON escaping.
    fixer = Fixer::new(JsonEscaping::No);

    // Test various addresses outside `main`, `f`, and `g`, using
    // `example-windows.pdb` directly.
    let mut outside = |addr| {
        let line = format!("#00: foobar[tests/example-windows.pdb +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(format!("#00: foobar (tests/example-windows.pdb)"), line);
    };
    outside(0x0); // A very low address.
    outside(0x999); // Well before the start of main.
    outside(0x6bbf); // One byte before the start of `main`.
    outside(0x6be7); // One byte past the end of `main`.
    outside(0xfffffff); // A very high address.
}

#[test]
fn test_regex() {
    let mut fixer = Fixer::new(JsonEscaping::No);

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

    // An error message is also printed to stderr for file errors, but we don't
    // test for that.
    unchanged("#00: ???[tests/no-such-file +0x43a0]"); // No such file.
    unchanged("#00: ???[src/main.rs +0x43a0]"); // File exists, but has wrong format.

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
