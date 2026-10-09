# BASIC-SPECS.md - ZXBASIC Dialect Specification

Standardized language specification for the **zxbasic** dialect of Sinclair BASIC. This document defines the grammar, keyword semantics, operator precedence, type system, and runtime behaviors. It serves as the single source of truth for cross-implementation compatibility across any target language or architecture.

---

## 1. Lexical Conventions

### 1.1 Source Encoding & Lines
* **Character Set:** UTF-8 / ASCII (7-bit clean subset required for program statements).
* **Line Number:** An unsigned integer in the range `1` to `9999`. Lines must be executed in strictly ascending numerical order regardless of input order.
* **Line Structure:** `[<line_number>] <statement> [ : <statement> ... ] [ \n | \r\n ]`
* **Statement Separator:** The colon (`:`) separates multiple statements on a single line.
* **Case Sensitivity:**
  * Keywords and built-in functions are **case-insensitive** (`PRINT`, `print`, `Print` are identical).
  * Variable identifiers are **case-sensitive** (`a` and `A` designate distinct variables, adhering to Sinclair convention).

### 1.2 Comments
* Initiated by the `REM` keyword. All characters following `REM` up to the end of the physical line are ignored by the executor. Colons inside or after a `REM` do not terminate the comment.

---

## 2. Type System & Identifiers

### 2.1 Numeric Type
* All numbers are IEEE 754 double-precision 64-bit floating-point values.
* Boolean values follow Sinclair logic:
  * False is represented as `0`.
  * True is represented as `1`.
  * Any non-zero numeric evaluation in a conditional context evaluates as True.
* Integers (used for array subscripts, line targets, and character codes) are derived by truncating or rounding to the nearest integer toward zero (`INT` semantics).

### 2.2 String Type
* Dynamic-length strings of bytes (0 to 65,535 bytes).
* String literals are enclosed in double quotes (`"Hello, world!"`).
* To represent a double quote within a string literal, two consecutive double quotes are used (`"He said, ""Hello!"""`).

### 2.3 Identifier Rules
* **Numeric Variables:** Begin with an alphabetic character (`A`-`Z`, `a`-`z`), optionally followed by alphanumeric characters.
* **String Variables:** Begin with an alphabetic character, optionally followed by alphanumeric characters, and end immediately with a dollar sign (`A$`, `b$`, `NAME$`, `line2$`). Like numeric variables, string variable names are case-sensitive (`name$` and `NAME$` are distinct).
* **Numeric Arrays:** An identifier followed by dimension parentheses (e.g., `A(10)`, `matrix(3, 3)`).
* **String Arrays:** An identifier ending in `$` followed by dimensions (e.g., `A$(10, 32)`). Unlike the Spectrum's fixed-width character matrices, every element is an independent dynamic-length string (initially `""`), and all dimensions are element indices. When a string array exists, `A$(i)` refers to its element; otherwise `A$(i)` is the single character `A$(i TO i)` of the string variable `A$`.

---

## 3. Operator Precedence & Expressions

Expressions are evaluated according to the following precedence hierarchy (from highest to lowest). Operators on the same line associate left-to-right, except exponentiation which associates right-to-left.

| Precedence | Operators | Description |
| :--- | :--- | :--- |
| **1 (Highest)** | Functions, Slicing, `( )` | Function calls, array indexing, `A$(start TO end)` |
| **2** | `^` | Exponentiation (right-associative) |
| **3** | `+`, `-` (unary) | Unary positive and negation |
| **4** | `*`, `/` | Multiplication, Division |
| **5** | `+`, `-` (binary) | Addition, String Concatenation (`+`), Subtraction |
| **6** | `=`, `<>`, `<`, `>`, `<=`, `>=` | Relational comparisons (return `0` or `1`) |
| **7** | `NOT` | Logical NOT |
| **8** | `AND` | Logical AND (Sinclair shortcut: returns left operand if right is true, else 0) |
| **9 (Lowest)** | `OR` | Logical OR (Sinclair shortcut: returns left operand if true, else right) |

### 3.1 Sinclair String Slicing
Substrings use 1-based indices via the `TO` keyword within parentheses:
* `A$(s TO e)`: Slice from index `s` through index `e` (inclusive).
* `A$(s TO)`: Slice from index `s` to the end of `A$`.
* `A$(TO e)`: Slice from index `1` through index `e`.
* `A$(i)`: Returns the single-character string at index `i` (identical to `A$(i TO i)`).
* *Out of bounds handling:* If `s > e`, the result is an empty string `""`. If indices exceed the string length, raise error `3 Subscript out of range`.

---

## 4. Statements & Commands

### 4.1 Program Lifecycle & Environment
* **`NEW`**
  * Clears the current program store, all variables, arrays, and execution stacks.
* **`CLEAR`**
  * Clears all variables and runtime stacks without deleting program lines.
* **`RUN [line_number]`**
  * Resets runtime state (clears variables and call stacks), resets the `DATA` pointer to the first line, and begins execution at `line_number` (or the lowest line number if omitted).
* **`LIST [line_number]`**
  * Emits program lines in ascending order to standard output. If `line_number` is supplied, listing begins at or immediately after that line.
* **`STOP`**
  * Halts program execution immediately and emits report code `9 STOP statement`.
* **`SAVE <string_expr>`**
  * Serializes the program store as plain UTF-8 text to the specified filesystem path.
* **`LOAD <string_expr>`**
  * Implicitly executes `NEW`, reads the plain text file from the path, parses line numbers, and populates the program store.

### 4.2 Assignment & Memory
* **`LET <var> = <expr>`**
  * Assigns evaluated expression to variable. Explicit `LET` is mandatory.
  * Assigning to a substring (`LET A$(2 TO 4)="xy"`, `LET A$(3)="z"`) replaces those characters in place, padding the new text with spaces or truncating it to the slice length (Sinclair "Procrustean" assignment).
* **`DIM <name>(<dim1> [, <dim2> ...])`**
  * Allocates a numeric or string array. Arrays use 1-based indexing. Elements start as `0` or `""`. Dimensions are truncated to integers and must be at least `1` (else `B Integer out of range`). Arrays above 10,000,000 elements raise `4 Out of memory`. Re-dimensioning an existing array raises `C Nonsense in BASIC`.

### 4.3 Control Flow
* **`GOTO <expr>`**
  * Transfers execution to the line number resulting from rounding `<expr>`. A target outside `1`–`9999` raises `B Integer out of range`; a target line that does not exist raises `N Statement lost`. The same rules apply to `GOSUB <expr>` and `RUN <line_number>`.
* **`GOSUB <expr>`**
  * Pushes the address of the next statement onto the call stack and jumps to `<expr>`.
* **`RETURN`**
  * Pops the top statement location from the call stack and resumes execution. Raises `7 Return without GOSUB` if stack is empty.
* **`IF <expr> THEN <statement>`**
  * Evaluates `<expr>`. If non-zero (true), executes the `<statement>` (and any remaining statements on that line). If zero (false), execution skips the remainder of the physical line.
* **`FOR <var> = <start> TO <limit> [STEP <step>]`**
  * Initializes loop variable `<var>` to `<start>`. Evaluates `<limit>` and default `<step>` (`1` if omitted). Pushes loop bounds to the loop stack, replacing any active loop on the same variable.
  * If the loop would not run even once (`<start> > <limit>` with a non-negative step, or `<start> < <limit>` with a negative step), execution continues after the matching `NEXT <var>` (searched forward from the `FOR`). If there is no matching `NEXT`, `1 NEXT without FOR` is raised.
* **`NEXT <var>`**
  * Increments `<var>` by its recorded `STEP`. If `<step> >= 0` and `<var> <= <limit>`, or `<step> < 0` and `<var> >= <limit>`, execution loops back to the statement following `FOR`. Otherwise, loop state is popped and execution continues. A `NEXT` with no active loop on `<var>` raises `1 NEXT without FOR`.

### 4.4 Data Blocks
* **`DATA <val1> [, <val2> ...]`**
  * Declares static literals (numbers or raw/quoted strings) compiled into a linear data pool. Skipped during normal execution.
  * Unquoted items are trimmed raw text. Read into a numeric variable, an unquoted item is evaluated as a numeric expression. Reading a quoted item into a numeric variable raises `C Nonsense in BASIC`.
* **`READ <var1> [, <var2> ...]`**
  * Reads the next sequential value from the `DATA` pool into the given variable. Raises `8 End of DATA` if read past available items.
* **`RESTORE [line_number]`**
  * Resets the `DATA` reading cursor to the beginning of the program, or to the first `DATA` item at or following `line_number`.

### 4.5 Console Input/Output
* **`CLS`**
  * Clears the terminal screen and moves cursor to the home position (`\033[2J\033[H`), resetting output column counter to 0.
* **`PRINT [<item> [separator] ...]`**
  * Output items include numbers, strings, and separator tokens:
    * `;` (Semicolon): Concatenates output with zero separation.
    * `,` (Comma): Advances cursor to the next 16-character column tabstop.
    * `'` (Apostrophe): Forces a newline (`\n`).
  * If a `PRINT` statement does not end with `;` or `,`, a newline is appended automatically.
* **`INPUT [ <prompt_str> ; ] <var> [ <separator> ... ]`**
  * Reads a line of user input from the terminal and assigns it to `<var>`, optionally printing `<prompt_str>` first. Several prompts and variables may be mixed with `;`, `,` and `'` separators (`INPUT "Name? "; n$, "Age? "; age`); each variable reads one line.
  * A reply for a numeric variable is evaluated as a numeric expression (like `VAL`), so `2*3` and other variables are accepted.

---

## 5. Built-in Functions

### 5.1 Mathematical Functions
| Function | Signature | Description |
| :--- | :--- | :--- |
| `ABS(x)` | `f64 -> f64` | Absolute value |
| `ACS(x)` | `f64 -> f64` | Arccosine in radians |
| `ASN(x)` | `f64 -> f64` | Arcsine in radians |
| `ATN(x)` | `f64 -> f64` | Arctangent in radians |
| `COS(x)` | `f64 -> f64` | Cosine of angle in radians |
| `EXP(x)` | `f64 -> f64` | Exponential $e^x$ |
| `INT(x)` | `f64 -> f64` | Floor (largest integer $\le x$) |
| `LN(x)` | `f64 -> f64` | Natural logarithm ($\ln x$) |
| `RND` | `-> f64` | Pseudo-random float $r$ where $0.0 \le r < 1.0$ |
| `SGN(x)` | `f64 -> f64` | Signum: `-1` if $x < 0$, `0` if $x = 0$, `1` if $x > 0$ |
| `SIN(x)` | `f64 -> f64` | Sine of angle in radians |
| `SQR(x)` | `f64 -> f64` | Square root ($\sqrt{x}$) |
| `TAN(x)` | `f64 -> f64` | Tangent of angle in radians |

### 5.2 String & Conversion Functions
| Function | Signature | Description |
| :--- | :--- | :--- |
| `CHR$(n)` | `f64 -> str` | Returns single-character string from ASCII code `n` |
| `CODE(s)` | `str -> f64` | Returns ASCII code of the first character in `s` (or 0 for `""`) |
| `LEN(s)` | `str -> f64` | Length of string `s` in characters |
| `STR$(n)` | `f64 -> str` | Converts number `n` to its string representation |
| `VAL(s)` | `str -> f64` | Evaluates string `s` as a numeric expression |
| `INKEY$` | `-> str` | Non-blocking console read (returns `""` if no key is pending) |

---

## 6. Standard Error Codes & Diagnostics

When execution halts or statement parsing fails, implementations must output diagnostic reports adhering to the standard Sinclair error codes:

```text
<Code> <Description> [, <Line Number>:<Statement Index>]
```

* `<Statement Index>` is the 1-based position of the offending statement within the line (statements are separated by `:`).
* The `, <Line Number>:<Statement Index>` suffix is omitted for errors raised by commands entered in immediate mode.

| Code | Message | Trigger Condition |
| --- | --- | --- |
| `0` | `OK` | Normal termination / successful completion |
| `1` | `NEXT without FOR` | `NEXT` with no active loop on that variable, or a skipped loop with no matching `NEXT` |
| `2` | `Variable not found` | Reading an unassigned variable or non-existent array |
| `3` | `Subscript out of range` | Array index or string slice bounds exceeded |
| `4` | `Out of memory` | Memory limit reached during allocation (array size, string over 65,535 characters, GOSUB nesting over 10,000) |
| `6` | `Number too big` | Division by zero or a result too large for a double |
| `7` | `Return without GOSUB` | Encountering `RETURN` with an empty subroutine stack |
| `8` | `End of DATA` | Reading past the final available `DATA` element |
| `9` | `STOP statement` | Execution of a `STOP` instruction |
| `A` | `Invalid argument` | Function argument outside its domain (`SQR` of a negative, `LN` of a non-positive, `ASN`/`ACS` outside `-1..1`) |
| `B` | `Integer out of range` | Line number outside `1..9999`, invalid array dimension, `CHR$` outside `0..255` |
| `C` | `Nonsense in BASIC` | Syntax or type error, or an immediate-only command used in a program |
| `F` | `File not found` / `File error` | `LOAD` of a missing file, or another `SAVE`/`LOAD` failure |
| `H` | `STOP in INPUT` | End of input while `INPUT` is waiting |
| `L` | `BREAK into program` | Ctrl+C pressed while a program runs, or at an `INPUT` |
| `N` | `Statement lost` | Jump to a line that does not exist |
