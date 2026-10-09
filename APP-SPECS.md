# APP-SPECS.md - zxbasic (Modern Rust Sinclair BASIC Interpreter)

Specification and bootstrapping contract for building a lightweight ZX Spectrum BASIC interpreter for POSIX terminals in Rust.

---

## 1. Project Overview & Scope

* **Target:** Linux (x86_64 / aarch64, Debian 13 "Trixie" on ChromeOS Crostini).
* **Executable Name:** `zxbasic`
* **Language:** Rust, edition 2024 (minimum supported Rust version: 1.85, matching the Debian Trixie `rustc` package). Stable toolchain only.
* **Code Quality:** `#![forbid(unsafe_code)]` at the crate root. Code must be clean under `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`.
* **Design principle:** Clean UNIX/POSIX terminal utility. No Sinclair hardware emulation (no ULA emulation, no display RAM attributes, no token-keyboard mapping).
* **Dependencies:** Rust standard library only, except for:
  * [`rustyline`](https://crates.io/crates/rustyline) for REPL line-editing. A custom raw-mode editor is out of scope because termios access would need `unsafe` code or another crate.
  * [`nix`](https://crates.io/crates/nix) (features `term` and `poll` only) for safe termios/poll wrappers used by BREAK and `INKEY$` (§2.4). It is already a dependency of `rustyline`, so it adds no new crate to the build.

  No heavy GUI/audio dependencies. Every added crate must be justified in `Cargo.toml` comments.

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
pub enum Input {
    Line(String),
    Interrupted, // Ctrl+C
    Eof,         // Ctrl+D or end of piped input
}

pub trait LineSource {
    /// Reads one line; `initial` pre-populates the editor (used by `EDIT`).
    fn read_line(&mut self, prompt: &str, initial: &str) -> std::io::Result<Input>;
    /// Whether prompts are shown and `EDIT` is available.
    fn is_interactive(&self) -> bool;
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
  * `INPUT` statements read the next line from the same stdin stream. The `INPUT` prompt is program output and is written to stdout. A numeric reply that cannot be evaluated reports `C Nonsense in BASIC`, and end of input during `INPUT` reports `H STOP in INPUT`.
  * `EDIT` is unavailable and reports `C Nonsense in BASIC`. `AUTO` still works: lines are numbered automatically and a blank line ends `AUTO` mode.

### 2.4 Running Programs

* **Reports:** Reports are written to stderr, on a fresh line (a newline is first written to stdout if the cursor is not at column 0). In interactive mode, a program that runs to completion reports `0 OK, <line>:<statement>`; in non-interactive mode success is silent.
* **BREAK (interactive mode only):** While statements execute, the terminal is switched to non-canonical, no-echo, no-signal mode (`nix::sys::termios`), so Ctrl+C arrives as a byte rather than killing the process. Pending input is polled every 256 statements (`nix::poll`) and Ctrl+C stops the program with `L BREAK into program, <line>:<statement>`. The original terminal settings are restored when execution ends, before each `INPUT`, and when the `Console` is dropped.
* **`INKEY$`:** Returns the next key pressed while the program runs, or `""` if none is pending. In non-interactive mode it always returns `""`.
* **`INPUT` (interactive):** Reads through `rustyline` with the `INPUT` prompt. Ctrl+C reports `L BREAK into program`, and a numeric reply that cannot be evaluated re-prompts.
* **`CLS`:** Emits `\033[2J\033[H` only when stdout is a terminal, so piped output stays clean.
* **Program changes during a run:** `NEW` or `LOAD` executed from a program line ends the run.

### 2.5 Command Line

```text
zxbasic [FILE]
zxbasic -h | --help
```

* **No arguments:** starts the REPL on stdin (interactive or non-interactive, per §2.3).
* **`FILE`:** loads the program as `LOAD "FILE"` would, then runs it as `RUN` would. When the program ends (normally, at `STOP`, on an error or on BREAK), its report is printed and the REPL continues on stdin. The program and its variables are kept, so it can be inspected, edited or re-run.
  * A line in the file that fails verification is reported (`C Nonsense in BASIC, <line>:<statement>`) and skipped, exactly as with `LOAD`, and the remaining program still runs.
  * `EXIT n` in the program terminates the process with status `n` immediately, without entering the REPL.
  * If the file cannot be read, `zxbasic: <FILE>: <message>` (e.g. `File not found`) is written to stderr and the process exits with status `1` without starting the REPL.
  * Combined with non-interactive stdin, this gives a scripting mode: `zxbasic prog.bas < /dev/null` runs the program and exits with status `0` at end of input.
* **`-h`, `--help`:** prints usage to stdout and exits with status `0`.
* **Usage errors** (unknown option, more than one `FILE`): an error and the usage text are written to stderr; exit status `2`.
* Paths are taken as raw OS strings (`std::env::args_os`), so non-UTF-8 file names work.

---

## 3. Language Dialect Specification

### 3.1 Syntax & Formatting

* Case-insensitive keyword parsing (`PRINT`, `print`, `Print`).
* Multi-statement lines delimited by colon (`:`), e.g., `10 LET x=1 : PRINT x`.
* In-line comments initiated with `REM`.

### 3.2 Types & Expressions

* **Numbers:** Standard IEEE 754 64-bit float (`f64`).
* **Strings:** Identifiers ending with `$` (e.g., `A$`, `NAME$`); multi-letter names are allowed (see BASIC-SPECS.md §2.3). Stored as `String`. `LEN`, `CODE` and 1-based slicing operate on characters (not bytes), so UTF-8 text in string literals slices safely. Out-of-range slices raise `3 Subscript out of range` rather than panicking.
* **Type checking:** Expression types are checked statically when a line is parsed, so `LET a="x"` is rejected on entry with `C Nonsense in BASIC`.
* **Number output (`PRINT`, `STR$`):** Spectrum style. Integers below 10^13 print in full. Other values are rounded to 8 significant digits, with no leading zero before the decimal point (`.5`, `-.25`). `E` notation (`1E+20`, `1.5E-7`) is used outside the 10^-5 to 10^13 range.
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


* `SAVE "<path>"`: Serializes the current `Program` as `<line> <text>` lines into the designated file path (`std::fs::write`).
* `LOAD "<path>"`: Reads the whole file first, so a missing or unreadable file leaves the current program untouched. It then invokes `NEW` (clearing variables and the existing program store) and runs each line through the line-store insertion logic. Blank lines are skipped. A line that is unnumbered or fails syntax verification is reported (`C Nonsense in BASIC`), and loading continues with the next line.
* `std::io::Error` values are mapped to `F File not found` (`ErrorKind::NotFound`) or `F File error` (anything else); I/O failures must not terminate the REPL.

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicError {
    pub code: ErrorCode,
    pub line: Option<LineNo>, // None in immediate mode
    pub statement: u16,       // 1-based statement index within the line
}
```

The full list of codes, with their trigger conditions, is in BASIC-SPECS.md §6.

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
│   ├── main.rs        # Binary: argument parsing; wires stdin/stdout/stderr and the console into the REPL
│   ├── lib.rs         # Library root: module declarations & public API
│   ├── repl.rs        # Prompt loop, AUTO numbering, EDIT pre-population
│   ├── input.rs       # LineSource: rustyline (TTY) / plain stdin (non-TTY)
│   ├── terminal.rs    # Console: run-mode terminal settings, BREAK, INKEY$
│   ├── program.rs     # Line store (insert, delete, list, iterate)
│   ├── lexer.rs       # Tokenizer, keyword recognizer, keyword canonicalization
│   ├── ast.rs         # Statement and expression syntax tree
│   ├── parser.rs      # Statement/expression parser with static type checking
│   ├── interpreter.rs # Execution engine, variables, stacks, DATA pool, I/O
│   ├── format.rs      # Spectrum-style number formatting
│   ├── renum.rs       # RENUM line mapping and branch-target patching
│   └── error.rs       # BasicError / ErrorCode and report formatting
├── examples/          # Sample programs (also run by the test suite)
└── tests/
    ├── store.rs       # Integration tests for line management
    ├── interpreter.rs # End-to-end scripts run through the REPL (piped mode)
    └── cli.rs         # Runs the zxbasic binary: FILE argument, exit statuses, usage
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
# REPL line editing / in-memory history.
# Default features are disabled to drop `with-file-history` (no history file support compiled in).
rustyline = { version = "18.0.1", default-features = false }
# Safe termios/poll wrappers for BREAK (Ctrl+C) and INKEY$ while a program runs.
# Already a dependency of rustyline, so this adds no new crate to the build.
nix = { version = "0.31", default-features = false, features = ["poll", "term"] }

[profile.release]
opt-level = 3
debug = true
```

* Standard workflow (replaces Makefile targets):
  * `cargo build` / `cargo build --release` — builds `zxbasic` (replaces `make all`).
  * `cargo test` — runs unit and integration tests (replaces `make test`).
  * `cargo clean` — removes build artifacts (replaces `make clean`).
  * `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` — required to pass before committing.
