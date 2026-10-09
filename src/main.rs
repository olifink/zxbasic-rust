//! zxbasic REPL entry point and input dispatcher.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use zxbasic::input::{self, Input};
use zxbasic::{BasicError, ErrorCode, Program};

const PROMPT: &str = "> ";

fn main() -> ExitCode {
    let mut source = match input::open() {
        Ok(source) => source,
        Err(e) => {
            eprintln!("zxbasic: cannot open input: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut program = Program::new();

    loop {
        let prompt = if source.is_interactive() { PROMPT } else { "" };
        match source.read_line(prompt, "") {
            Ok(Input::Line(line)) => {
                if let Err(err) = dispatch(&mut program, &line) {
                    eprintln!("{err}");
                }
            }
            Ok(Input::Interrupted) => continue,
            Ok(Input::Eof) => return ExitCode::SUCCESS,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                eprintln!("{}", BasicError::immediate(ErrorCode::Nonsense));
            }
            Err(e) => {
                eprintln!("zxbasic: read error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
}

/// Stores numbered lines; runs everything else immediately.
fn dispatch(program: &mut Program, line: &str) -> Result<(), BasicError> {
    let line = line.trim_start();
    if line.is_empty() {
        return Ok(());
    }

    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        let line_no = line[..digits]
            .parse::<u32>()
            .map_err(|_| BasicError::immediate(ErrorCode::IntegerOutOfRange))
            .and_then(Program::validate_line_no)?;
        let text = line[digits..].trim();
        if text.is_empty() {
            program.delete(line_no);
        } else {
            program.insert(line_no, text.to_string());
        }
        return Ok(());
    }

    // Placeholder until the lexer/parser exist: only LIST and NEW are understood.
    match line.trim().to_ascii_uppercase().as_str() {
        "LIST" => {
            for (n, text) in program.iter() {
                println!("{n} {text}");
            }
            Ok(())
        }
        "NEW" => {
            program.clear();
            Ok(())
        }
        _ => Err(BasicError::immediate(ErrorCode::Nonsense)),
    }
}
