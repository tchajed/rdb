use std::{
    env,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
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

fn cmd_stdout(out: Output) -> String {
    String::from_utf8(out.stdout).expect("non utf-8 output")
}

fn run_rdb(lines: &[&str]) -> String {
    let mut cmd = Command::new(exe_path("rdb"))
        .arg(exe_path("test"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to launch debugger");

    let mut stdin = cmd.stdin.take().expect("couldn't get stdin");
    let input = lines.iter().map(|&x| x).collect::<Vec<_>>().join("\n");
    std::thread::spawn(move || {
        stdin
            .write_all(input.as_bytes())
            .expect("could not write to rdb");
    });

    let output = cmd.wait_with_output().expect("couldn't get stdout");
    cmd_stdout(output)
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
    // at least for now, quitting runs the program (when the debugger terminates)
    assert!(out.contains("Hello, world"), "target output did not appear");
}

#[test]
fn test_help() {
    let out = run_rdb(&["help", "quit"]);
    assert!(out.contains("supported commands:"));
}

#[test]
fn test_dump() {
    let out = run_rdb(&["register dump", "quit"]);
    assert!(out.contains("rip"));
}
