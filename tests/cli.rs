//! Command-line tests: run the `zxbasic` binary with arguments and piped stdin.

use std::io::Write;
use std::process::{Command, Stdio};

struct Run {
    out: String,
    err: String,
    status: i32,
}

fn zxbasic(args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_zxbasic"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start zxbasic");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    Run {
        out: String::from_utf8(output.stdout).unwrap(),
        err: String::from_utf8(output.stderr).unwrap(),
        status: output.status.code().unwrap_or(-1),
    }
}

fn temp_program(name: &str, text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("zxbasic-cli-{}-{name}.bas", std::process::id()));
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn file_argument_loads_and_runs() {
    let r = zxbasic(&["examples/fibonacci.bas"], "");
    assert_eq!(r.out, "Fibonacci Series:\n0 1 1 2 3 5 8 13 21 34 \n");
    assert_eq!(r.err, "9 STOP statement, 120:1\n");
    assert_eq!(r.status, 0);
}

#[test]
fn repl_continues_with_program_and_variables() {
    let r = zxbasic(
        &["examples/arrays.bas"],
        "PRINT \"[\";names$(2);\"]\"\nLIST 160\n",
    );
    assert!(r.out.ends_with("[BOB       ]\n160 STOP\n"), "{}", r.out);
}

#[test]
fn exit_in_program_ends_the_process() {
    let path = temp_program("exit", "10 PRINT \"bye\"\n20 EXIT 4\n");
    let r = zxbasic(&[path.to_str().unwrap()], "PRINT \"not reached\"\n");
    std::fs::remove_file(&path).unwrap();
    assert_eq!((r.out.as_str(), r.status), ("bye\n", 4));
}

#[test]
fn bad_lines_are_reported_and_the_rest_runs() {
    let path = temp_program("bad", "10 PRINT \"ok\"\n20 PRINT (\n");
    let r = zxbasic(&[path.to_str().unwrap()], "");
    std::fs::remove_file(&path).unwrap();
    assert_eq!(r.out, "ok\n");
    assert_eq!(r.err, "C Nonsense in BASIC, 20:1\n");
}

#[test]
fn missing_file_fails() {
    let r = zxbasic(&["no/such/file.bas"], "PRINT 1\n");
    assert_eq!(r.out, "");
    assert_eq!(r.err, "zxbasic: no/such/file.bas: File not found\n");
    assert_eq!(r.status, 1);
}

#[test]
fn usage_errors() {
    let r = zxbasic(&["a.bas", "b.bas"], "");
    assert!(
        r.err.starts_with("zxbasic: only one FILE may be given\n"),
        "{}",
        r.err
    );
    assert_eq!(r.status, 2);

    let r = zxbasic(&["--bogus"], "");
    assert!(
        r.err.starts_with("zxbasic: unknown option '--bogus'\n"),
        "{}",
        r.err
    );
    assert_eq!(r.status, 2);
}

#[test]
fn help() {
    let r = zxbasic(&["--help"], "");
    assert!(r.out.starts_with("Usage: zxbasic [FILE]"), "{}", r.out);
    assert_eq!(r.status, 0);
}

#[test]
fn no_arguments_reads_stdin() {
    let r = zxbasic(&[], "10 PRINT 6*7\nRUN\n");
    assert_eq!((r.out.as_str(), r.status), ("42\n", 0));
}
