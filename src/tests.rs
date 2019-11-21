// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::*;

#[test]
fn test_linux() {
    let mut fixer = Fixer::new(JsonEscaping::No);

    // The debuginfo within `example-linux` is as follows.
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

    // Test various different unchanged line forms.
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

    // Test various different changed line forms.
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
fn test_json_linux() {
    // The debuginfo within `example-json-linux` is as follows.
    //
    //   FUNC 0x1130 size=45 func=main
    //   LINE 0x1130 line=16 file=/home/njn/moz/fix-stacks/tests/example"json.c
    //   LINE 0x113f line=17 file=/home/njn/moz/fix-stacks/tests/example"json.c
    //   LINE 0x1155 line=18 file=/home/njn/moz/fix-stacks/tests/example"json.c

    let line = "#00: ???[tests/example-json-linux +0x1130]";

    // Test without JSON escaping.
    let mut fixer = Fixer::new(JsonEscaping::No);
    let expected = "#00: main (/home/njn/moz/fix-stacks/tests/example\"json.c:16)";
    assert_eq!(expected, fixer.fix(line.to_string()));

    // Test with JSON escaping.
    let mut fixer = Fixer::new(JsonEscaping::Yes);
    let expected = "#00: main (/home/njn/moz/fix-stacks/tests/example\\\"json.c:16)";
    assert_eq!(expected, fixer.fix(line.to_string()));
}
