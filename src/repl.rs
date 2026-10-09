//! Interactive shell: reads lines, handles `AUTO` numbering and `EDIT`
//! pre-population, and passes everything else to the interpreter.

use std::io::ErrorKind;
use std::path::Path;

use crate::error::{BasicError, ErrorCode};
use crate::input::{Input, LineSource};
use crate::interpreter::{Interpreter, Outcome};
use crate::program::{LineNo, MAX_LINE};

const PROMPT: &str = "> ";

struct Auto {
    next: LineNo,
    step: LineNo,
}

pub struct Repl {
    interpreter: Interpreter,
    source: Box<dyn LineSource>,
    auto: Option<Auto>,
    initial: String,
}

impl Repl {
    pub fn new(interpreter: Interpreter, source: Box<dyn LineSource>) -> Self {
        Repl {
            interpreter,
            source,
            auto: None,
            initial: String::new(),
        }
    }

    /// Loads and runs a program file, as `LOAD` followed by `RUN`, leaving
    /// the program and its variables in place for the REPL that follows.
    /// Returns an exit status if the program executed `EXIT`.
    pub fn load_and_run(&mut self, path: &Path) -> Result<Option<i32>, BasicError> {
        self.interpreter.load(path)?;
        Ok(self.handle("RUN"))
    }

    /// Runs until end of input or `EXIT`; returns the process exit status.
    pub fn run(&mut self) -> i32 {
        loop {
            let prompt = self.prompt();
            let initial = std::mem::take(&mut self.initial);
            match self.source.read_line(&prompt, &initial) {
                Ok(Input::Line(line)) => {
                    if let Some(code) = self.handle(&line) {
                        return code;
                    }
                }
                Ok(Input::Interrupted) => self.auto = None,
                Ok(Input::Eof) => return 0,
                Err(e) if e.kind() == ErrorKind::InvalidData => {
                    self.interpreter
                        .report(&BasicError::immediate(ErrorCode::Nonsense));
                }
                Err(e) => {
                    eprintln!("zxbasic: read error: {e}");
                    return 1;
                }
            }
        }
    }

    fn prompt(&self) -> String {
        match &self.auto {
            Some(auto) => {
                let marker = if self.interpreter.program().get(auto.next).is_some() {
                    "*"
                } else {
                    ""
                };
                format!("{}{marker} ", auto.next)
            }
            None => PROMPT.to_string(),
        }
    }

    /// Handles one input line; returns an exit status to stop the REPL.
    fn handle(&mut self, line: &str) -> Option<i32> {
        if let Some(auto) = &self.auto {
            if line.trim().is_empty() {
                self.auto = None;
                return None;
            }
            let (next, step) = (auto.next, auto.step);
            match self.interpreter.store_line(&format!("{next} {line}")) {
                Ok(()) => {
                    let following = u32::from(next) + u32::from(step);
                    self.auto = (following <= u32::from(MAX_LINE)).then_some(Auto {
                        next: following as LineNo,
                        step,
                    });
                }
                // A rejected line keeps its number so it can be retyped.
                Err(e) => self.interpreter.report(&e),
            }
            return None;
        }

        match self.interpreter.enter_line(line, self.source.as_mut()) {
            Outcome::Continue => {}
            Outcome::Exit(code) => return Some(code),
            Outcome::Edit(text) => self.initial = text,
            Outcome::Auto { start, step } => {
                self.auto = Some(Auto { next: start, step });
            }
        }
        None
    }
}
