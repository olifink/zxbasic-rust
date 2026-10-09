//! zxbasic REPL entry point.

#![forbid(unsafe_code)]

use std::io::{self, IsTerminal};
use std::process::ExitCode;

use zxbasic::terminal::Console;
use zxbasic::{Interpreter, Output, Repl, input};

fn main() -> ExitCode {
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

    let code = Repl::new(interpreter, source).run();
    // The REPL (and with it the console) is dropped here, restoring the terminal.
    ExitCode::from(code as u8)
}
