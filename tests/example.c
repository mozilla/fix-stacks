// The source code for the `example` tests.
//
// `example-linux` was compiled on an Ubuntu 19.04 box using clang 8.0 with the
// following command:
//
//   clang -g example.rs -o example-linux
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
