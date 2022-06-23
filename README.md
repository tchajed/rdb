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
debugging pid 73833
rdb> break 0x55555555baf1
rdb> continue
Hello, world
stopped at breakpoint
rdb> register write rip 0x55555555bac4
rdb> continue
Hello, world
stopped at breakpoint
rdb> continue
program exited
rdb> quit
```

The first breakpoint is just after `println!("Hello, world")`, so we see the
program print. The `register write rip` command rewinds the instruction
pointer, which causes the print to run again and reach the same breakpoint as before.
