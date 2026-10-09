//! zxbasic: a lightweight Sinclair ZX Spectrum BASIC interpreter.
//!
//! See `APP-SPECS.md`, `BASIC-SPECS.md` and `SPECS-v2.md` for the specification.

#![forbid(unsafe_code)]

pub mod error;
pub mod input;
pub mod lexer;
pub mod parser;
pub mod program;

pub use error::{BasicError, ErrorCode};
pub use program::{LineNo, Program};
