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
use executables and other data files created by compilers as inputs.

The primary source code file used for generating these files is
`tests/example.c`. The following files were generated from it.
- `example-linux`: produced on an Ubuntu 19.04 box by clang 8.0 with the
  command `clang -g example.c -o example-linux`.
- `example-windows` and `example-windows.pdb`: produced on a Windows 10 laptop
  by clang 9.0 with the command `clang -g example.c -o example-windows`.
  `example-windows` was then hex-edited to change the PDB reference from the
  absolute path `c:\Users\njn\moz\fix-stacks\tests\example-windows.pdb` to the
  relative path `tests/////////////////////////////example-windows.pdb`. (The
  use of many redundant forward slashes is a hack to keep the path the same
  length, which avoids the need for more complex changes to that file.)

## Obtaining the debug info

The unit tests refer to specific addresses within the generated binaries. These
addresses were determined for each generated executable by uncommenting the
`eprintln!` statements in `FileInfo::new` and running `fix-stacks` on an input
that mentions that executable. For example, the debug info for `example-linux`
was obtained using an input file containing this line:
```
#00: ???[tests/example-linux +0x0]
```
The resulting `FUNC` and `LINE` lines can be seen in `src/tests.rs`.

