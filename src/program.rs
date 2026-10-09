//! Program line store (APP-SPECS.md §2.1).

use std::collections::BTreeMap;

use crate::error::{BasicError, ErrorCode};

/// Line number in the range 1..=9999.
pub type LineNo = u16;

pub const MIN_LINE: LineNo = 1;
pub const MAX_LINE: LineNo = 9999;

#[derive(Debug, Default)]
pub struct Program {
    /// Raw source text, stripped of the leading line number.
    lines: BTreeMap<LineNo, String>,
}

impl Program {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks that `line_no` lies within 1..=9999.
    pub fn validate_line_no(line_no: u32) -> Result<LineNo, BasicError> {
        match LineNo::try_from(line_no) {
            Ok(n) if (MIN_LINE..=MAX_LINE).contains(&n) => Ok(n),
            _ => Err(BasicError::immediate(ErrorCode::IntegerOutOfRange)),
        }
    }

    /// Inserts or replaces `line_no`.
    pub fn insert(&mut self, line_no: LineNo, source: String) {
        self.lines.insert(line_no, source);
    }

    /// Removes `line_no`, returning its source if it existed.
    pub fn delete(&mut self, line_no: LineNo) -> Option<String> {
        self.lines.remove(&line_no)
    }

    pub fn get(&self, line_no: LineNo) -> Option<&str> {
        self.lines.get(&line_no).map(String::as_str)
    }

    /// All lines in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = (LineNo, &str)> {
        self.lines.iter().map(|(n, s)| (*n, s.as_str()))
    }

    /// Lines at or after `start`, in ascending order (`LIST n`, `RESTORE n`).
    pub fn iter_from(&self, start: LineNo) -> impl Iterator<Item = (LineNo, &str)> {
        self.lines.range(start..).map(|(n, s)| (*n, s.as_str()))
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Removes every line (`NEW`).
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Replaces the whole program at once (`RENUM`).
    pub fn replace_all(&mut self, lines: BTreeMap<LineNo, String>) {
        self.lines = lines;
    }
}
