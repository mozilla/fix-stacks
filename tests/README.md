# Notes on tests

## Constraints

The stack frames produced by `MozFormatCodeAddress()` contain absolute paths
and refer to build files, which means that `fix-stacks` can only be sensibly
run on the same machine that produced the stack frames.

However, the test inputs must work on any machine, not just the machine that
produced those inputs. Furthermore, it is convenient when developing if all the
tests works on all platforms, e.g. the tests involving ELF/DWARF files should
work on Windows, and the tests involving PE/PDB files should work on Linux.

To allow this requires the following.
- All paths in inputs must be relative, rather than absolute.
- All paths must use forward slashes rather than backslashes as directory
  separators. (This is because Windows allows both forward slashes and
  backslashes, but Linux and Mac only allow forward slashes.) This includes the
  paths in text inputs, and also some paths within executables (such as a PE
  file's reference to a PDB file).

## Generating inputs

Debug info is very complicated and hard to write by hand. Therefore, the tests
use executables and other data files created by compilers as inputs. These were
all generated within the `test` directory in the following ways.

### Linux

`example-linux` was produced on an Ubuntu 19.04 box by clang 8.0 with this
command within `tests/`:
```
clang -g example.c -o example-linux
```

### Windows

`example-windows.exe` and `example-windows.pdb` were produced on a Windows 10
 laptop by clang 9.0 with this command within `tests/`:
```
clang -g example.c -o example-windows.exe
```
`example-windows.exe` was then hex-edited to change the PDB reference from the
absolute path `c:\Users\njn\moz\fix-stacks\tests\example-windows.pdb` to the
relative path `tests/////////////////////////////example-windows.pdb`. (The use
of many redundant forward slashes is a hack to keep the path the same length,
which avoids the need for more complex changes to that file.)

### Mac

The Mac tests are more complex because `fix-stacks`'s code for handling Mach-O
binaries is more complex than other formats.

`mac-multi` was produced on a MacBook Pro running macOS 10.14 by Apple clang
11.0 with these commands within `tests/`:
```
# A normal file.
clang -c -g mac-normal.c -o mac-normal.o
# A fat binary.
clang -m32 -c -g mac-fat.c -o mac-fat-32.o
clang -m64 -c -g mac-fat.c -o mac-fat-64.o
lipo -create mac-fat-32.o mac-fat-64.o -output mac-fat.o
# A library.
clang -c -g mac-lib1.c -o mac-lib1.o
clang -c -g mac-lib2.c -o mac-lib2.o
ar -r libexample.a mac-lib1.o mac-lib2.o
# The final executable.
clang mac-normal.o mac-fat.o libexample.a -o mac-multi
```
`mac-multi` was then hex-edited to change all the file reference from the
absolute paths such as `/Users/njn/moz/fix-stacks/tests/mac-normal.c` to the
relative paths such as `tests///////////////////////////mac-normal.c`. (The use
of many redundant forward slashes is a hack to keep the path the same length,
which avoids the need for more complex changes to that file.)

### Breakpad symbols

`bpsyms/example-linux/` was produced on an Ubuntu 19.10 box by `dump_syms`
(from a development build of Firefox), with these commands within `tests/`:
```
# 123456781234567812345678123456789 is a fake UUID whose exact value doesn't
# matter.
DIR="bpsyms/example-linux/123456781234567812345678123456789/"
mkdir $DIR
# $OBJDIR is the object directory of the Firefox build.
$OBJDIR/dist/host/bin/dump_syms example-linux > $DIR/example-linux.sym
```

`bpsyms/example-windows.pdb/` was produced on Windows 10 laptop by
`dump_syms.exe`, with these commands within `tests/`:
```
# $SRCDIR is the source directory of a Firfox build.
$SRCDIR/mach artifact toolchain --from-build win64-dump-syms
# 123456781234567812345678123456789 is a fake UUID whose exact value doesn't
# matter.
DIR="bpsyms/example-windows.pdb/123456781234567812345678123456789/"
mkdir $DIR
dump_syms/dump_syms.exe example-windows.exe > $DIR/example-windows.sym
```

## Obtaining the debug info

The unit tests refer to specific addresses within the generated binaries. These
addresses were determined for each generated executable by changing the
`PRINT_FUNCS_AND_LINES` constant to `true` and running `fix-stacks` on an input
that mentions that executable. For example, the debug info for `example-linux`
was obtained using an input file containing this line:
```
#00: ???[tests/example-linux +0x0]
```
The resulting `FUNC` and `LINE` lines can be seen in `src/tests.rs`.

