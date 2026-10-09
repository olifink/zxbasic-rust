# APP-SPECS.md - zxbasic (Modern Rust Sinclair BASIC Interpreter)

Specification and bootstrapping contract for building a lightweight ZX Spectrum BASIC interpreter for POSIX terminals in Rust.

---

## 1. Project Overview & Scope

* **Target:** Linux (x86_64 / aarch64, Debian 13 "Trixie" on ChromeOS Crostini).
* **Executable Name:** `zxbasic`
* **Language:** Rust, edition 2024 (minimum supported Rust version: 1.85, matching the Debian Trixie `rustc` package). Stable toolchain only.
* **Code Quality:** `#![forbid(unsafe_code)]` at the crate root. Code must be clean under `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`.
* **Design principle:** Clean UNIX/POSIX terminal utility. No Sinclair hardware emulation (no ULA emulation, no display RAM attributes, no token-keyboard mapping).
* **Dependencies:** Rust standard library only, except for REPL line-editing: [`rustyline`](https://crates.io/crates/rustyline). A custom raw-mode editor is out of scope because termios access would need `unsafe` code or another crate. No heavy GUI/audio dependencies. Every added crate must be justified in `Cargo.toml` comments.

---

## 2. System Architecture

The executable operates as an interactive shell executing or storing lines based on the presence of a leading integer line number:


```
                           +---------------------------+
                           |     Terminal / Stdin      |
                           +---------------------------+
                                         |
                                  [Interactive REPL]
                                         |
                       +-----------------+-----------------+
                       |                                   |
              [Has Line Number?]                  [No Line Number]
                       |                                   |
             +---------v----------+              +---------v----------+
             | Program Store      |              | Immediate Executor |
             | (BTreeMap<u16,..>) |              | (Evaluate & Run)   |
             +--------------------+              +--------------------+
                       ^                                   |
                       |              [RUN]                |
                       +-----------------------------------+
                                         |
                                 +-------v-------+
                                 | Runtime State |
                                 | - Variables   |
                                 | - GOSUB Stack |
                                 | - FOR Loops   |
                                 | - DATA/RESTORE|
                                 +---------------+

```

### 2.1 Program Store
* **Line Range:** `1` to `9999`.
* **Data Structure:** An ordered map keyed by line number, which keeps lines in ascending order without manual sorting:

```rust
use std::collections::BTreeMap;

/// Line number in the range 1..=9999.
pub type LineNo = u16;

#[derive(Debug, Default)]
pub struct Program {
    /// Raw source text, stripped of the leading line number.
    lines: BTreeMap<LineNo, String>,
}

impl Program {
    pub fn insert(&mut self, line_no: LineNo, source: String) { /* insert or replace */ }
    pub fn delete(&mut self, line_no: LineNo) -> Option<String> { /* remove */ }
    pub fn get(&self, line_no: LineNo) -> Option<&str> { /* lookup */ }
    pub fn iter(&self) -> impl Iterator<Item = (LineNo, &str)> { /* ascending */ }
    pub fn iter_from(&self, start: LineNo) -> impl Iterator<Item = (LineNo, &str)> { /* LIST n, RESTORE n */ }
    pub fn clear(&mut self) { /* NEW */ }
}
```

* **Store Operations:**
  * `<number> <text>`: Inserts or replaces `line_no` (`BTreeMap::insert`).
  * `<number>`: (No trailing text) Deletes the line matching `line_no` (`BTreeMap::remove`).
  * Line numbers outside `1..=9999` are rejected with `B Integer out of range`.



### 2.2 Execution Engine

* **Direct Commands:** Executed immediately without being saved (`LIST`, `RUN`, `NEW`, `CLEAR`, `SAVE`, `LOAD`).
* **Runtime Program Counter (PC):** A position value `(LineNo, statement_index)`; the next line is found with `BTreeMap::range((Excluded(line), Unbounded))`. GOSUB and FOR stacks store the same position type.
* **Variable Table:** `HashMap<String, f64>` for numbers and `HashMap<String, String>` for strings (keys are case-sensitive identifiers).
* **Values:** Expression results are represented as an enum:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Str(String),
}
```

* **Error Propagation:** All fallible operations return `Result<T, BasicError>` and use `?`. No `panic!`, `unwrap()` or `expect()` on user-controlled input; a BASIC error must never crash the interpreter process.

### 2.3 Input Modes (TTY vs. Non-TTY)

At startup the REPL checks `std::io::stdin().is_terminal()` (`std::io::IsTerminal`, no extra crate) and selects one of two input sources behind a common interface:

```rust
pub trait LineSource {
    /// Returns `Ok(None)` at end of input.
    fn read_line(&mut self, prompt: &str, initial: &str) -> std::io::Result<Option<String>>;
}
```

* **Interactive (stdin is a TTY):** `rustyline::Editor<(), MemHistory>` (explicit in-memory history type, independent of crate features).
  * Shows the prompt, supports cursor movement, history, and `EDIT` pre-population (`readline_with_initial`).
  * History is kept in memory for the current session only. `zxbasic` never reads or writes a history file and creates no files or directories of its own; only an explicit `SAVE` writes to disk.
  * `Ctrl+C` (`ReadlineError::Interrupted`) cancels the current input line (or exits `AUTO` mode) and returns to the prompt; it does not exit.
  * `Ctrl+D` on an empty line (`ReadlineError::Eof`) exits with status `0`.
* **Non-interactive (stdin is a pipe or file, e.g. `zxbasic < prog.bas` or `cat prog.bas | zxbasic`):** plain `std::io::stdin().lock()` via `BufRead::read_line`, with no `rustyline` involvement.
  * No prompt, banner, or line-editor escape sequences are written to stdout, so program output can be piped or diffed cleanly.
  * Each input line is processed exactly as if typed: numbered lines go to the program store, unnumbered lines run immediately (so a script can end with `RUN`).
  * Trailing `\n` / `\r\n` is stripped. Invalid UTF-8 is reported as `C Nonsense in BASIC` for that line and processing continues.
  * Reports are written to stderr, and processing continues with the next input line.
  * End of input exits with status `0`. Scripts that need a different status use `EXIT n` (SPECS-v2.md §2.3).
  * `INPUT` statements read the next line from the same stdin stream.
  * `EDIT` is unavailable and reports `C Nonsense in BASIC`. `AUTO` still works: lines are numbered automatically and a blank line ends `AUTO` mode.

---

## 3. Language Dialect Specification

### 3.1 Syntax & Formatting

* Case-insensitive keyword parsing (`PRINT`, `print`, `Print`).
* Multi-statement lines delimited by colon (`:`), e.g., `10 LET x=1 : PRINT x`.
* In-line comments initiated with `REM`.

### 3.2 Types & Expressions

* **Numbers:** Standard IEEE 754 64-bit float (`f64`).
* **Strings:** Identifiers ending with `$` (e.g., `A$`, `NAME$`); multi-letter names are allowed (see BASIC-SPECS.md §2.3). Stored as `String`; program text is restricted to the 7-bit ASCII subset, so 1-based slicing maps directly to byte ranges. Slicing must use checked access (`str::get`) and raise `3 Subscript out of range` rather than panicking.
* **Operators:**
  * Arithmetic: `+`, `-`, `*`, `/`, `^` (exponentiation, `f64::powf`).
  * Relational: `=`, `<>`, `<`, `>`, `<=`, `>=`.
  * Logical: `AND`, `OR`, `NOT`.


* **Sinclair String Slicing:** 1-based indexing syntax:
  * `A$(start TO end)`, `A$(start TO)`, `A$(TO end)`.



### 3.3 Statement Support (Phase Breakdown)

* **Phase 1 (Bootstrap):** `LIST`, `RUN`, `NEW`, `CLEAR`, `CLS`, `PRINT` (strings and numeric literals), `LET` (assignment).
* **Phase 2 (Control Flow):** `GOTO`, `IF ... THEN`, `STOP`, `INPUT`.
* **Phase 3 (Loops & Subroutines):** `FOR ... TO ... STEP`, `NEXT`, `GOSUB`, `RETURN`, `DATA`, `READ`, `RESTORE`.
* **Phase 4 (Persistence & Functions):**
  * File I/O: `SAVE "file.bas"`, `LOAD "file.bas"`.
  * Math: `INT`, `ABS`, `SGN`, `SQR`, `RND`, `SIN`, `COS`, `TAN`.
  * Strings: `LEN`, `STR$`, `VAL`, `CHR$`, `CODE`.
* **`RND`:** Implemented with a small built-in PRNG (e.g., xorshift64*) seeded from `SystemTime`, to avoid a dependency on the `rand` crate.



---

## 4. File I/O (SAVE / LOAD)

* Plain UTF-8 text files (`.bas`).
* Each line in the file matches standard ASCII text output:
```text
10 REM Simple Loop
20 FOR i=1 TO 5
30 PRINT "Index: "; i
40 NEXT i

```


* `SAVE "<path>"`: Serializes the current `Program` sequentially into the designated file path (`std::fs::File` + `BufWriter`, one `writeln!` per line).
* `LOAD "<path>"`: Invokes `NEW` (clears variables and existing program store), reads lines sequentially from the file (`BufReader::lines`), and runs them through the line-store insertion logic.
* `std::io::Error` values are mapped to a `BasicError` report; I/O failures must not terminate the REPL.

---

## 5. Sinclair Error Codes

Output Sinclair-style report format to standard error/output upon error:

```text
<Report Code> <Message>, <Line Number>:<Statement Index>

```

Errors are modelled as a single enum implementing `std::fmt::Display` and `std::error::Error` (hand-written impls; no `thiserror` dependency required):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Ok,                  // 0 OK
    VariableNotFound,    // 2 Variable not found
    SubscriptOutOfRange, // 3 Subscript out of range
    ReturnWithoutGosub,  // 7 Return without GOSUB
    EndOfData,           // 8 End of DATA
    Stop,                // 9 STOP statement
    IntegerOutOfRange,   // B Integer out of range
    Nonsense,            // C Nonsense in BASIC
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicError {
    pub code: ErrorCode,
    pub line: Option<LineNo>, // None in immediate mode
    pub statement: u16,       // 1-based statement index within the line
}
```

Primary codes to implement:

* `0 OK`
* `2 Variable not found`
* `3 Subscript out of range`
* `7 Return without GOSUB`
* `8 End of DATA`
* `9 STOP statement`
* `C Nonsense in BASIC` (General syntax error)

---

## 6. CLI Bootstrapping & File Structure

Target file layout for the project root (a single Cargo package with a library crate plus a thin binary, so the interpreter core is unit- and integration-testable):

```text
zxbasic/
├── Cargo.toml
├── Cargo.lock
├── APP-SPECS.md
├── BASIC-SPECS.md
├── SPECS-v2.md
├── src/
│   ├── main.rs       # Binary: REPL entry point and input dispatcher
│   ├── lib.rs        # Library root: module declarations & public API
│   ├── program.rs    # Line store (insert, delete, list, iterate)
│   ├── input.rs      # LineSource: rustyline (TTY) / plain stdin (non-TTY)
│   ├── lexer.rs      # Tokenizer and keyword recognizer
│   ├── parser.rs     # Expression parser & statement execution
│   └── error.rs      # BasicError / ErrorCode and report formatting
└── tests/
    └── store.rs      # Integration tests for line management
```

* Unit tests live alongside the code in `#[cfg(test)] mod tests { ... }` blocks; `tests/` holds integration tests that drive the library's public API.

### Initial Cargo Requirements

```toml
[package]
name = "zxbasic"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"

[dependencies]
# REPL line editing / in-memory history (only non-std dependency).
# Default features are disabled to drop `with-file-history` (no history file support compiled in).
rustyline = { version = "18.0.1", default-features = false }

[profile.release]
opt-level = 3
debug = true
```

* Standard workflow (replaces Makefile targets):
  * `cargo build` / `cargo build --release` — builds `zxbasic` (replaces `make all`).
  * `cargo test` — runs unit and integration tests (replaces `make test`).
  * `cargo clean` — removes build artifacts (replaces `make clean`).
  * `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` — required to pass before committing.
