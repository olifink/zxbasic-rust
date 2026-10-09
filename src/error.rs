//! Sinclair report codes and error formatting (APP-SPECS.md §5, BASIC-SPECS.md §6).

use std::fmt;
use std::io;

use crate::program::LineNo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Ok,                  // 0 OK
    NextWithoutFor,      // 1 NEXT without FOR
    VariableNotFound,    // 2 Variable not found
    SubscriptOutOfRange, // 3 Subscript out of range
    OutOfMemory,         // 4 Out of memory
    NumberTooBig,        // 6 Number too big
    ReturnWithoutGosub,  // 7 Return without GOSUB
    EndOfData,           // 8 End of DATA
    Stop,                // 9 STOP statement
    InvalidArgument,     // A Invalid argument
    IntegerOutOfRange,   // B Integer out of range
    Nonsense,            // C Nonsense in BASIC
    FileNotFound,        // F File not found
    FileError,           // F File error
    StopInInput,         // H STOP in INPUT
    Break,               // L BREAK into program
    StatementLost,       // N Statement lost
}

impl ErrorCode {
    /// The single-character report code shown before the message.
    pub fn code(self) -> char {
        match self {
            ErrorCode::Ok => '0',
            ErrorCode::NextWithoutFor => '1',
            ErrorCode::VariableNotFound => '2',
            ErrorCode::SubscriptOutOfRange => '3',
            ErrorCode::OutOfMemory => '4',
            ErrorCode::NumberTooBig => '6',
            ErrorCode::ReturnWithoutGosub => '7',
            ErrorCode::EndOfData => '8',
            ErrorCode::Stop => '9',
            ErrorCode::InvalidArgument => 'A',
            ErrorCode::IntegerOutOfRange => 'B',
            ErrorCode::Nonsense => 'C',
            ErrorCode::FileNotFound | ErrorCode::FileError => 'F',
            ErrorCode::StopInInput => 'H',
            ErrorCode::Break => 'L',
            ErrorCode::StatementLost => 'N',
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            ErrorCode::Ok => "OK",
            ErrorCode::NextWithoutFor => "NEXT without FOR",
            ErrorCode::VariableNotFound => "Variable not found",
            ErrorCode::SubscriptOutOfRange => "Subscript out of range",
            ErrorCode::OutOfMemory => "Out of memory",
            ErrorCode::NumberTooBig => "Number too big",
            ErrorCode::ReturnWithoutGosub => "Return without GOSUB",
            ErrorCode::EndOfData => "End of DATA",
            ErrorCode::Stop => "STOP statement",
            ErrorCode::InvalidArgument => "Invalid argument",
            ErrorCode::IntegerOutOfRange => "Integer out of range",
            ErrorCode::Nonsense => "Nonsense in BASIC",
            ErrorCode::FileNotFound => "File not found",
            ErrorCode::FileError => "File error",
            ErrorCode::StopInInput => "STOP in INPUT",
            ErrorCode::Break => "BREAK into program",
            ErrorCode::StatementLost => "Statement lost",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicError {
    pub code: ErrorCode,
    pub line: Option<LineNo>, // None in immediate mode
    pub statement: u16,       // 1-based statement index within the line
}

impl BasicError {
    /// An error not (yet) tied to a program position.
    pub fn immediate(code: ErrorCode) -> Self {
        BasicError {
            code,
            line: None,
            statement: 1,
        }
    }

    /// An error raised at `line:statement` of the stored program.
    pub fn at(code: ErrorCode, line: LineNo, statement: u16) -> Self {
        BasicError {
            code,
            line: Some(line),
            statement,
        }
    }

    /// Maps an I/O failure of `SAVE`/`LOAD` to a report.
    pub fn from_io(e: &io::Error) -> Self {
        let code = match e.kind() {
            io::ErrorKind::NotFound => ErrorCode::FileNotFound,
            _ => ErrorCode::FileError,
        };
        BasicError::immediate(code)
    }
}

impl From<ErrorCode> for BasicError {
    fn from(code: ErrorCode) -> Self {
        BasicError::immediate(code)
    }
}

impl fmt::Display for BasicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code.code(), self.code.message())?;
        if let Some(line) = self.line {
            write!(f, ", {}:{}", line, self.statement)?;
        }
        Ok(())
    }
}

impl std::error::Error for BasicError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_program_error_with_position() {
        let err = BasicError::at(ErrorCode::Stop, 20, 2);
        assert_eq!(err.to_string(), "9 STOP statement, 20:2");
    }

    #[test]
    fn formats_immediate_error_without_position() {
        let err = BasicError::immediate(ErrorCode::Nonsense);
        assert_eq!(err.to_string(), "C Nonsense in BASIC");
    }
}
