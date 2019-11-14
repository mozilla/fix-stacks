use crate::Fixer;

#[test]
fn test1() {
    let mut fixer = Fixer::new();

    // The debuginfo within `example` is as follows.
    //
    // FUNCTIONS
    //
    //   addr   size  mangled-name
    //   ----   ----  ------------
    //   0x43a0 440   _ZN7example4main17h28effd2b7f9f10b9E
    //
    // LINES
    //
    //   addr   line  file
    //   ----   ----  ----
    //   0x43a0   22  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x43a7   23  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x43b4    0  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x43b9   24  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x43d3    0  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x43df   15  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x43ea   16  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x43f4    9  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x4419   10  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x4456   17  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x4472    0  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x447e   17  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x4489   18  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x4493    9  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x44b8   10  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x44f5   19  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x450f   15  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x4521   17  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x4533   19  /home/njn/moz/fix-stacks/tests/example.rs
    //   0x4550   25  /home/njn/moz/fix-stacks/tests/example.rs
    //
    // Yes, rustc really does produce debuginfo with some 0 line numbers. See
    // https://github.com/rust-lang/rust/issues/65487 for more.

    // Test various addresses within `main`.
    let mut main = |addr, linenum| {
        let line = format!("#00: ???[tests/example +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(
            line,
            format!(
                "#00: example::main (/home/njn/moz/fix-stacks/tests/example.rs:{})",
                linenum
            )
        );
    };
    main(0x43a0, 22);
    main(0x43a1, 22);
    main(0x43a2, 22);
    main(0x43a3, 22);
    main(0x43a4, 22);
    main(0x43a5, 22);
    main(0x43a6, 22);
    main(0x43a7, 23);
    main(0x43b4, 0);
    main(0x43bd, 24);
    main(0x43f4, 9);
    main(0x4400, 9);
    main(0x4500, 19);
    main(0x450f, 15);
    main(0x4550, 25);
    main(0x4550, 25);
    main(0x4557, 25);

    // Try a new Fixer.
    fixer = Fixer::new();

    // Test various addresses outside `main`.
    let mut outside = |addr| {
        let line = format!("#00: ???[tests/example +0x{:x}]", addr);
        let line = fixer.fix(line);
        assert_eq!(line, format!("#00: ??? (tests/example)",));
    };
    outside(0x0); // Well before the start of main.
    outside(0x999); // Well before the start of main.
    outside(0x439f); // One byte before the start of `main`.
    outside(0x4558); // One byte past the end of `main`.
    outside(0xfffffff); // Well past the end of main.

    // Test various different unchanged line forms.
    let mut unchanged = |line: &str| {
        let line2 = fixer.fix(line.to_string());
        assert_eq!(line, line2);
    };
    unchanged("");
    unchanged("1234 ABCD");
    unchanged("00: ???[tests/example +0x43a0]"); // Missing the leading '#'.
    unchanged("#00: ???[tests/example 0x43a0]"); // Missing the '+' before the address.
    unchanged("#00: ???[tests/example +43a0]"); // Missing the '0x`.
    unchanged("#00: ???(tests/example +0x43a0)"); // Wrong parentheses.

    // An error message is also printed to stderr for file errors, but we don't
    // test for that.
    unchanged("#00: ???[tests/EXAMPLE +0x43a0]"); // No such file.
    unchanged("#00: ???[src/main.rs +0x43a0]"); // File exists, but has wrong format.

    // Test various different changed line forms.
    let mut changed = |line1: &str, line2_expected| {
        let line2_actual = fixer.fix(line1.to_string());
        assert_eq!(line2_expected, line2_actual);
    };
    changed(
        "#01: foobar[tests/example +0x43a0]",
        "#01: example::main (/home/njn/moz/fix-stacks/tests/example.rs:22)",
    );
    changed(
        "PREFIX#9999: ???[tests/example +0x43a0]SUFFIX",
        "PREFIX#9999: example::main (/home/njn/moz/fix-stacks/tests/example.rs:22)SUFFIX",
    );
    changed(
        "#01: ???[tests/../src/../tests/example +0x43a0]",
        "#01: example::main (/home/njn/moz/fix-stacks/tests/example.rs:22)",
    );
}
