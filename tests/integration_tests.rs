use std::{
    env,
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
};

fn exe_path(name: &str) -> PathBuf {
    let bin_dir = env::current_exe()
        .unwrap()
        .parent()
        .expect("test executable's directory")
        .parent()
        .expect("output directory")
        .to_path_buf();
    bin_dir.join(name)
}

fn wait_stdout(cmd: Child) -> String {
    let out = cmd.wait_with_output().expect("couldn't get stdout");
    String::from_utf8(out.stdout).expect("non utf-8 output")
}

fn spawn_rdb() -> Child {
    Command::new(exe_path("rdb"))
        .arg(exe_path("test"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to launch debugger")
}

fn run_rdb(lines: &[&str]) -> String {
    let mut cmd = spawn_rdb();

    let mut stdin = cmd.stdin.take().expect("couldn't get stdin");
    let input = lines.to_vec().join("\n");
    std::thread::spawn(move || {
        stdin
            .write_all(input.as_bytes())
            .expect("could not write to rdb");
    });

    wait_stdout(cmd)
}

#[test]
fn test_continue() {
    let out = run_rdb(&["continue", "quit"]);
    assert!(out.contains("Hello, world"), "target output did not appear");
    assert!(out.contains("program exited"), "target didn't terminate");
}

#[test]
fn test_quit() {
    let out = run_rdb(&["quit"]);
    assert!(!out.contains("Hello, world"), "target should not have run");
}

#[test]
fn test_help() {
    let out = run_rdb(&["help", "quit"]);
    assert!(out.contains("SUBCOMMANDS:"));
}

#[test]
fn test_dump() {
    let out = run_rdb(&["register dump", "quit"]);
    assert!(out.contains("rip"));
}

#[test]
fn test_eof() {
    let mut cmd = spawn_rdb();
    let stdin = cmd.stdin.take().expect("couldn't get stdin");
    // equivalent to sending EOF (ctrl-d)
    drop(stdin);

    // this only terminates if the debugger exits
    let out = wait_stdout(cmd);
    assert!(!out.contains("Hello"), "program should not have run");
}

#[test]
fn test_single_stepping() {
    run_rdb(&["stepi", "stepi", "stepi", "quit"]);
}

#[test]
fn test_step_out_main() {
    // at the very beginning finish should return from main
    let out = run_rdb(&["finish"]);
    assert!(out.contains("program exited"));
}

#[test]
fn test_function_breakpoint() {
    let out = run_rdb(&["break use_vars", "continue", "next", "quit"]);
    assert!(out.contains(">      let mut b: u64 = 2;"));
}

#[test]
fn test_function_finish() {
    let out = run_rdb(&["break use_vars", "continue", "next", "finish", "quit"]);
    assert!(out.contains(">      greeting();"));
}

#[test]
fn test_function_step_in() {
    let out = run_rdb(&["break use_vars", "continue", "finish", "step", "quit"]);
    assert!(out.contains("fn greeting()"));
}
