# SPECS-v2.md - zxbasic Phase 2 Enhancements

Specification for interactive developer ergonomics, line normalization, syntax gating, and program transformation.

---

## 1. Line Ingestion & Normalization

When a user enters a line starting with a line number (`<number> <text>`), the line must pass through two pre-storage filter passes: **Canonical Capitalization** and **Syntax Verification**.

### 1.1 Keyword Auto-Capitalization
* **Rule:** Keywords and built-in function names outside string literals and `REM` comments are converted to uppercase upon ingestion into the program store.
* **Preservation Constraints:**
  * String literals (`"..."`) retain their exact casing.
  * `REM` contents (from `REM` up to the end of the line) retain exact casing.
  * Variable identifiers retain case sensitivity (`total` remains `total`, `A$` remains `A$`), but reserved keywords (`print`, `for`, `to`, `step`, `goto`, `gosub`, `inkey$`, etc.) become canonical uppercase (`PRINT`, `FOR`, `TO`, `STEP`, `GOTO`, `GOSUB`, `INKEY$`).
* **Example:**
  * Input: `10 for i=1 to 10: print "count: "; i: next i`
  * Stored: `10 FOR i=1 TO 10: PRINT "count: "; i: NEXT i`

### 1.2 Ingestion-Time Syntax Verification
* **Behavior:** Every numbered line must be validated by the statement parser before updating the program store.
* **Failure Handling:**
  * If the parser encounters a syntax violation (e.g., mismatched quotes, unclosed parentheses, missing `THEN` in `IF`, invalid keyword combinations), the line is **rejected**.
  * The existing program store is left untouched (an existing line with the same number is **not** overwritten or deleted).
  * The interpreter emits standard Sinclair report:
    ```text
    C Nonsense in BASIC, <line_number>:<statement_index>
    ```

---

## 2. Interactive Environment Commands

These commands operate strictly in **Immediate Mode** (direct console entry). If encountered during program runtime (from a stored line), raise report code `C Nonsense in BASIC`.

### 2.1 `EDIT <line>`
* **Syntax:** `EDIT <line_number>`
* **Behavior:**
  * Looks up `<line_number>` in the program store. If absent, reports `B Integer out of range`.
  * Pre-populates the interactive line editor (e.g., `rustyline::Editor::readline_with_initial(prompt, (text, ""))`) with the line text formatted as:
    ```text
    <line_number> <source_text>
    ```
  * Places the cursor at the end of the line for immediate editing, backspacing, or re-entry.
  * Requires an interactive terminal. When stdin is not a TTY, `EDIT` reports `C Nonsense in BASIC` (see APP-SPECS.md §2.3).

### 2.2 `AUTO [line [, step]]`
* **Syntax:** `AUTO [start_line [, step]]`
* **Defaults:**
  * `start_line` defaults to `10` (or `10` if omitted).
  * `step` defaults to `10` (or `10` if omitted).
  * Examples: `AUTO` (10, 20, 30...), `AUTO 100` (100, 110, 120...), `AUTO 100, 5` (100, 105, 110...).
* **Behavior:**
  * Puts the REPL into automatic numbering mode.
  * The REPL automatically prefixes the prompt with `<current_line> ` (with trailing space).
  * If the user enters content, the line is ingested and `<current_line>` advances by `step`.
  * If `<current_line>` already exists in memory, an asterisk or marker may be displayed (e.g., `10* `) to alert overwrite.
  * **Exit Condition:** Submitting a blank line (pressing `Enter` on an empty line) or pressing `Ctrl+C` (`rustyline::error::ReadlineError::Interrupted`) exits `AUTO` mode and returns to standard REPL prompt.

### 2.3 `EXIT [n]`
* **Syntax:** `EXIT [code_expr]`
* **Behavior:**
  * Cleanly terminates the `zxbasic` interpreter process and exits to the host shell.
  * Evaluates `<code_expr>` as an integer exit status returned via `std::process::exit(n)` (after flushing stdout).
  * Defaults to `0` if `<code_expr>` is omitted (`EXIT` $\rightarrow$ `std::process::exit(0)`).
  * Valid in both immediate mode and inside programs (allows scripted shell termination from BASIC).

---

## 3. Program Renumbering: `RENUM`

Performs an in-place transformation of all stored lines and rewrites target branch constants.

### 3.1 Syntax & Defaults
* **Syntax:** `RENUM [new_start [, step]]`
* **Defaults:**
  * `new_start` defaults to `10`.
  * `step` defaults to `10`.
  * Maximum line number limit is `9999`. If renumbering would cause any line to exceed `9999`, the entire operation aborts with `B Integer out of range` and memory remains unmodified.

### 3.2 Algorithm
1. **Pass 1: Line Mapping Table**
   * Traverse existing program lines in ascending order.
   * Generate a translation map: `BTreeMap<LineNo, LineNo>` (old → new):
     $$\text{NewLine}_i = \text{new\_start} + i \times \text{step}$$
   * Verify all $\text{NewLine}_i \le 9999$.

2. **Pass 2: Patching Branch Targets**
   * Tokenize each line statement.
   * Detect numeric literals following branch keywords:
     * `GOTO <literal>`
     * `GOSUB <literal>`
     * `RESTORE <literal>`
   * If `<literal>` exists in `Map`, substitute `<literal>` with `Map[<literal>]`.
   * If `<literal>` is not found in `Map`, leave as-is and emit a warning:
     ```text
     Warning: Line reference <literal> not found at line <OldLine>
     ```
   * *Note on Expressions:* Dynamic targets (e.g., `GOTO x + 100`) cannot be rewritten and are left untouched.

3. **Pass 3: Rebuild Program Store**
   * Build the rewritten lines into a new `BTreeMap` and swap it into the program store only after all passes succeed (e.g., `std::mem::replace`), so a failure leaves memory unmodified.