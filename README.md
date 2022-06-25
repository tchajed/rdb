# rdb

A Linux debugger written in Rust. This is a project for learning Rust and about
debuggers. The implementation follows along with [Writing a Linux
Debugger](https://blog.tartanllama.xyz/writing-a-linux-debugger-setup/), porting the code over to Rust.

The main dependencies used are the libc crate to interact with the kernel using
`ptrace` (which is how the debugger controls the target process),
[rustyline](https://crates.io/crates/rustyline/) to make the command-line input
nicer to use, and clap to simplify parsing commands and arguments.

Here's an interesting session with the debugger, running a simple "Hello, world"
program ([test.rs](src/bin/test.rs)):

```
$ cargo build
$ ./target/debug/rdb ./target/debug/test
debugging pid 208686
rdb> break 0x79e0
rdb> continue
hit breakpoint 0x79e0
/home/tchajed/rdb/src/bin/test.rs:
   #[no_mangle]
>  fn use_vars() {
       let mut a: u64 = 3;
rdb> next
hit breakpoint 0x79e8
/home/tchajed/rdb/src/bin/test.rs:
   fn use_vars() {
>      let mut a: u64 = 3;
       let mut b: u64 = 2;
rdb> finish
hit breakpoint 0x7a19
/home/tchajed/rdb/src/bin/test.rs:
       use_vars();
>      greeting();
   }
rdb> step
/home/tchajed/rdb/src/bin/test.rs:
   #[no_mangle]
>  fn greeting() {
       println!("Hello, world");
rdb> continue
Hello, world
program exited
rdb> quit
```
