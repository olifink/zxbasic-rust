//! REPL input sources (APP-SPECS.md §2.3).
//!
//! Interactive terminals get a `rustyline` editor with in-memory, session-only
//! history; pipes and files are read as plain lines. Neither source creates
//! any files.

use std::io::{self, BufRead, IsTerminal};

use rustyline::error::ReadlineError;
use rustyline::history::MemHistory;
use rustyline::{Config, Editor};

/// Result of reading one line of input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Line(String),
    /// Ctrl+C: cancel the current line (or leave `AUTO` mode).
    Interrupted,
    /// End of input (Ctrl+D or EOF on a pipe).
    Eof,
}

pub trait LineSource {
    /// Reads one line. `initial` pre-populates the editor (used by `EDIT`).
    fn read_line(&mut self, prompt: &str, initial: &str) -> io::Result<Input>;

    /// Whether prompts are shown and `EDIT` is available.
    fn is_interactive(&self) -> bool;
}

/// Picks the interactive or plain source depending on whether stdin is a TTY.
pub fn open() -> io::Result<Box<dyn LineSource>> {
    if io::stdin().is_terminal() {
        Ok(Box::new(TerminalSource::new()?))
    } else {
        Ok(Box::new(PlainSource::new(io::stdin().lock())))
    }
}

/// `rustyline` editor with in-memory history only.
pub struct TerminalSource {
    editor: Editor<(), MemHistory>,
}

impl TerminalSource {
    pub fn new() -> io::Result<Self> {
        let config = Config::builder().auto_add_history(true).build();
        let editor = Editor::with_history(config, MemHistory::new()).map_err(to_io_error)?;
        Ok(TerminalSource { editor })
    }
}

impl LineSource for TerminalSource {
    fn read_line(&mut self, prompt: &str, initial: &str) -> io::Result<Input> {
        let result = if initial.is_empty() {
            self.editor.readline(prompt)
        } else {
            self.editor.readline_with_initial(prompt, (initial, ""))
        };
        match result {
            Ok(line) => Ok(Input::Line(line)),
            Err(ReadlineError::Interrupted) => Ok(Input::Interrupted),
            Err(ReadlineError::Eof) => Ok(Input::Eof),
            Err(e) => Err(to_io_error(e)),
        }
    }

    fn is_interactive(&self) -> bool {
        true
    }
}

/// Plain line reader for pipes and redirected files: no prompts, no editing.
pub struct PlainSource<R> {
    reader: R,
}

impl<R: BufRead> PlainSource<R> {
    pub fn new(reader: R) -> Self {
        PlainSource { reader }
    }
}

impl<R: BufRead> LineSource for PlainSource<R> {
    fn read_line(&mut self, _prompt: &str, _initial: &str) -> io::Result<Input> {
        let mut buf = Vec::new();
        if self.reader.read_until(b'\n', &mut buf)? == 0 {
            return Ok(Input::Eof);
        }
        if buf.ends_with(b"\n") {
            buf.pop();
            if buf.ends_with(b"\r") {
                buf.pop();
            }
        }
        String::from_utf8(buf)
            .map(Input::Line)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn is_interactive(&self) -> bool {
        false
    }
}

fn to_io_error(e: ReadlineError) -> io::Error {
    match e {
        ReadlineError::Io(e) => e,
        other => io::Error::other(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_source_strips_line_endings() {
        let mut src = PlainSource::new(&b"10 PRINT 1\r\n20 STOP\nLIST"[..]);
        assert_eq!(
            src.read_line("", "").unwrap(),
            Input::Line("10 PRINT 1".into())
        );
        assert_eq!(
            src.read_line("", "").unwrap(),
            Input::Line("20 STOP".into())
        );
        assert_eq!(src.read_line("", "").unwrap(), Input::Line("LIST".into()));
        assert_eq!(src.read_line("", "").unwrap(), Input::Eof);
    }

    #[test]
    fn plain_source_reports_invalid_utf8() {
        let mut src = PlainSource::new(&b"\xff\n10 REM ok\n"[..]);
        let err = src.read_line("", "").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            src.read_line("", "").unwrap(),
            Input::Line("10 REM ok".into())
        );
    }
}
