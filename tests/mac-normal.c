// This file contains the source code for some tests. If you change this file
// you may need to regenerate some test files. (Even changing the number of
// lines in this comment would have an effect.)
//
// See `tests/README.md` for more details.


#include <stdio.h>

static void duplicate() {
    printf("normal duplicate");
}

void lib1_B(int* x);
void lib2_B(int* x);

int main() {
    int x = 0;
    lib1_B(&x);
    lib2_B(&x);
    duplicate();
    return x;
}
