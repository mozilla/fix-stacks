// The source code for the `example"json` test, the name of which contains a
// special character that needs escaping in JSON output.
//
// `example-json-linux` was compiled on an Ubuntu 19.04 box using clang 8.0 with the
// following command:
//
//   clang -g example\"json.c -o example-json-linux
//

#include <stdio.h>

int main() {
    printf("hello");
    return 0;
}
