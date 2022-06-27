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

fn a() {
    // stop here
}

fn b() {
    a();
}

fn c() {
    a();
}

fn call_little_functions() {
    b();
    c();
}

fn main() {
    use_vars();
    greeting();
    call_little_functions();
}
