# fix-stacks

This program post-processes ("fixes") the stack frames produced by
`MozFormatCodeAddress()`, which often lack one or more of: function name, file
name, line number. It relies on the `symbolic` crate to read debug info from
files.

It reads from standard input and writes to standard output. Lines matching the
special stack frame format are modified appropriately. For example, this line:
```
#01: ???[tests/example +0x43a0]
```
is changed to this in the output:
```
#01: example::main (/home/njn/moz/fix-stacks/tests/example.rs:22)
```
Lines that do not match the special stack frame format are passed through
unchanged.

# Shortcomings

`fix-stacks` is Linux-only. We aim to eventually support
[Mac](https://github.com/mozilla/fix-stacks/issues/3),
[Windows](https://github.com/mozilla/fix-stacks/issues/4), and possibly
Android.

Use with debuginfo sections in separate files is untested and may not work.
