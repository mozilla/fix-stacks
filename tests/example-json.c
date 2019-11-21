// The source code for the JSON test, the name of which contains a special
// character that needs escaping in JSON output.
//
// `example-json-linux` was compiled on an Ubuntu 19.04 box using clang 8.0 with the
// following commands:
//
//   cp example-json.c example\"json.c
//   clang -g example\"json.c -o example-json-linux
//   rm example\"json.c
//
// The file is checked into the repository with the name example-json.c because
// the '"' character caused git checkout problems on Windows(!)

#include <stdio.h>

int main() {
    printf("hello");
    return 0;
}
