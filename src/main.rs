//! zxbasic entry point: `zxbasic [FILE]`.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use zxbasic::terminal::Console;
use zxbasic::{Interpreter, Output, Repl, input};

const USAGE: &str = "Usage: zxbasic [FILE]

Starts the Sinclair BASIC interpreter. With FILE, loads and runs that
program first, then continues at the prompt.";

fn main() -> ExitCode {
    let file = match parse_args(std::env::args_os().skip(1)) {
        Ok(Args::Run(file)) => file,
        Ok(Args::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("zxbasic: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let source = match input::open() {
        Ok(source) => source,
        Err(e) => {
            eprintln!("zxbasic: cannot open input: {e}");
            return ExitCode::FAILURE;
        }
    };
    let interactive = source.is_interactive();
    let console = if interactive {
        Console::for_stdin()
    } else {
        Console::disabled()
    };
    let out = Output::new(Box::new(io::stdout()), io::stdout().is_terminal());
    let interpreter = Interpreter::new(out, Box::new(io::stderr()), console, interactive);
    let mut repl = Repl::new(interpreter, source);

    if let Some(path) = file {
        match repl.load_and_run(&path) {
            Ok(None) => {}
            Ok(Some(code)) => return exit_code(code),
            Err(e) => {
                eprintln!("zxbasic: {}: {}", path.display(), e.code.message());
                return ExitCode::FAILURE;
            }
        }
    }

    let code = repl.run();
    // The REPL (and with it the console) is dropped on return, restoring the terminal.
    exit_code(code)
}

enum Args {
    Run(Option<PathBuf>),
    Help,
}

fn parse_args(args: impl Iterator<Item = OsString>) -> Result<Args, String> {
    let mut file = None;
    for arg in args {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Args::Help),
            Some(option) if option.starts_with('-') && option != "-" => {
                return Err(format!("unknown option '{option}'"));
            }
            _ if file.is_some() => return Err("only one FILE may be given".to_string()),
            _ => file = Some(PathBuf::from(arg)),
        }
    }
    Ok(Args::Run(file))
}

/// Exit statuses are truncated to 8 bits, as the shell would.
fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(code as u8)
}
