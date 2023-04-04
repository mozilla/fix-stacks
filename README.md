# fix-stacks

This program post-processes ("fixes") the stack frames produced by
`MozFormatCodeAddress()`, which often lack one or more of: function name, file
name, line number. It relies on the `symbolic` and `goblin` crates to read
debug info from files.

It reads from standard input and writes to standard output. Lines matching the
special stack frame format are modified appropriately. For example, a line
like this in the input:
```
#01: ???[tests/example +0x43a0]
```
is changed to something like this in the output:
```
#01: main (/home/njn/moz/fix-stacks/tests/example.c:24)
```
Lines that do not match the special stack frame format are passed through
unchanged.

By default, `fix-stacks` uses native debug info present in binary files. In
this case, because the stack frames produced by `MozFormatCodeAddress()` refer
to build files (such as libxul), `fix-stacks` must run on the same machine that
produced the stack frames and the build files. Furthermore, the build files
must not have changed since the stack frames were produced. Otherwise, source
locations in the output may be missing or incorrect.

Alternatively, you can use the `-b` option to tell `fix-stacks` to read
Breakpad symbol files, as packaged by Firefox. The argument must contain two
paths, separated by a comma: the first path points to the Breakpad symbols
directory, the second path points to the `fileid` executable in the Firefox
objdir. In this case, the processed output will contain square brackets instead
of parentheses, to make it detectable from the output that breakpad symbols
were used. 

`fix-stacks` works on Linux, Windows, and Mac.

# Shortcomings

On Linux, use with debuginfo sections in separate files is untested and
probably does not work.

# Android notes

In order to fix stacks in `logcat` output from an Android device, you need to
tell `fix-stacks` how to map the remote file paths to files on the host machine,
using the `--local` option.

The following command will check the file `objdir/dist/bin/<fileName>`
on the host machine for any files which can't be found at the original path.

```shell
adb logcat | fix-stacks --local objdir/dist/bin
```
