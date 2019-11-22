// This file contains the source code for the `example` tests. Multiple files
// are generated from this file. If you change this file at all you will
// probably need to regenerate those files. (Even changing the number of lines
// in this comment would have an effect.)
//
// See `tests/README.md` for more details.
//

#include <stdio.h>

static void g(int* x) {
    printf("hello");
    *x *= 2;
}

static void f(int* x) {
    *x += 1;
    g(x);
    *x += 1;
    g(x);
    *x += 1;
}

int main() {
    int x = 0;
    f(&x);
    return x;
}
