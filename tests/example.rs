// The source code for the `example` program. `example` was compiled on an
// Ubuntu 19.04 box using rustc 1.39.0 with the following command:
//
//   rustc -g example.rs
//

#[inline(always)]
fn g(x: &mut u32) {
    println!("hello");
    *x *= 2;
}

#[inline(always)]
fn f(x: &mut u32) {
    *x += 1;
    g(x);
    *x += 1;
    g(x);
    *x += 1;
}

fn main() {
    let mut x = 0;
    f(&mut x);
}
