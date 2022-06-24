#![allow(unused_variables, unused_mut, unused_assignments)]

#[no_mangle]
fn greeting() {
    println!("Hello, world");
}

#[no_mangle]
fn use_vars() {
    let mut a: u64 = 3;
    let mut b: u64 = 2;
    let c = a + b;
    a = 4;
}

fn main() {
    use_vars();
    greeting();
}
