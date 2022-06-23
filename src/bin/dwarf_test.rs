#![allow(unused_variables)]
#![allow(unused_assignments)]

#[no_mangle]
fn some_vars() {
    let mut a: u32 = 3;
    let b = 2;
    let c = a + b;
    a = 4;
}

fn main() {
    some_vars();
}
