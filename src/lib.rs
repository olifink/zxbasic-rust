//! zxbasic: a lightweight Sinclair ZX Spectrum BASIC interpreter.
//!
//! See `APP-SPECS.md`, `BASIC-SPECS.md` and `SPECS-v2.md` for the specification.

#![forbid(unsafe_code)]

pub mod ast;
pub mod error;
pub mod format;
pub mod input;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod program;
pub mod renum;
pub mod repl;
pub mod terminal;

pub use error::{BasicError, ErrorCode};
pub use interpreter::{Interpreter, Output};
pub use program::{LineNo, Program};
pub use repl::Repl;
