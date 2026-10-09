//! Statement execution and runtime state (APP-SPECS.md §2.2, BASIC-SPECS.md §4–§5).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::ops::Bound;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::{
    BinOp, DataItem, Expr, InputItem, LValue, PrintItem, Stmt, Subscript, Type, UnOp,
};
use crate::error::{BasicError, ErrorCode};
use crate::format::format_number;
use crate::input::{Input, LineSource};
use crate::lexer::{self, Keyword};
use crate::parser;
use crate::program::{LineNo, MIN_LINE, Program};
use crate::renum;
use crate::terminal::{Break, Console};

/// Maximum string length in characters (BASIC-SPECS.md §2.2).
const MAX_STRING_LEN: usize = 65_535;
/// Maximum number of elements in one array.
const MAX_ARRAY_ELEMENTS: usize = 10_000_000;
/// Maximum GOSUB nesting depth.
const MAX_GOSUB_DEPTH: usize = 10_000;
/// Statements executed between checks for BREAK.
const BREAK_CHECK_INTERVAL: u32 = 256;
/// Column width used by `,` in `PRINT` and `INPUT`.
const TAB_WIDTH: usize = 16;

type Result<T> = std::result::Result<T, BasicError>;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Str(String),
}

impl Value {
    fn num(self) -> Result<f64> {
        match self {
            Value::Number(n) => Ok(n),
            Value::Str(_) => Err(ErrorCode::Nonsense.into()),
        }
    }

    fn string(self) -> Result<String> {
        match self {
            Value::Str(s) => Ok(s),
            Value::Number(_) => Err(ErrorCode::Nonsense.into()),
        }
    }
}

/// What the REPL should do after a line has been entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Exit(i32),
    /// Pre-populate the editor with this text (`EDIT`).
    Edit(String),
    /// Enter automatic line numbering (`AUTO`).
    Auto {
        start: LineNo,
        step: LineNo,
    },
}

/// Program output with column tracking for `,` tab stops.
pub struct Output {
    writer: Box<dyn Write>,
    column: usize,
    is_terminal: bool,
}

impl Output {
    pub fn new(writer: Box<dyn Write>, is_terminal: bool) -> Self {
        Output {
            writer,
            column: 0,
            is_terminal,
        }
    }

    pub fn column(&self) -> usize {
        self.column
    }

    fn write_str(&mut self, s: &str) {
        // A closed pipe must not stop the program; output is simply dropped.
        let _ = self.writer.write_all(s.as_bytes());
        for c in s.chars() {
            if c == '\n' {
                self.column = 0;
            } else {
                self.column += 1;
            }
        }
    }

    fn newline(&mut self) {
        self.write_str("\n");
    }

    fn tab(&mut self) {
        let pad = TAB_WIDTH - self.column % TAB_WIDTH;
        self.write_str(&" ".repeat(pad));
    }

    fn cls(&mut self) {
        if self.is_terminal {
            let _ = self.writer.write_all(b"\x1b[2J\x1b[H");
        }
        self.column = 0;
    }

    fn flush(&mut self) {
        let _ = self.writer.flush();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Loc {
    Immediate,
    Line(LineNo),
}

/// A statement position: line (or the immediate line) and 0-based statement index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pos {
    loc: Loc,
    stmt: usize,
}

impl Pos {
    fn next(self) -> Pos {
        Pos {
            loc: self.loc,
            stmt: self.stmt + 1,
        }
    }

    fn line_start(line: LineNo) -> Pos {
        Pos {
            loc: Loc::Line(line),
            stmt: 0,
        }
    }
}

struct ForFrame {
    var: String,
    limit: f64,
    step: f64,
    body: Pos,
}

struct Array<T> {
    dims: Vec<usize>,
    data: Vec<T>,
}

impl<T: Clone> Array<T> {
    fn new(dims: Vec<usize>, fill: T) -> Result<Self> {
        let size = dims
            .iter()
            .try_fold(1usize, |acc, &d| acc.checked_mul(d))
            .filter(|&n| n <= MAX_ARRAY_ELEMENTS)
            .ok_or(ErrorCode::OutOfMemory)?;
        Ok(Array {
            dims,
            data: vec![fill; size],
        })
    }

    /// Offset of a 1-based multi-dimensional index.
    fn offset(&self, index: &[i64]) -> Result<usize> {
        if index.len() != self.dims.len() {
            return Err(ErrorCode::SubscriptOutOfRange.into());
        }
        let mut offset = 0;
        for (&i, &dim) in index.iter().zip(&self.dims) {
            if i < 1 || i as u64 > dim as u64 {
                return Err(ErrorCode::SubscriptOutOfRange.into());
            }
            offset = offset * dim + (i as usize - 1);
        }
        Ok(offset)
    }
}

#[derive(Default)]
struct Variables {
    nums: HashMap<String, f64>,
    strs: HashMap<String, String>,
    num_arrays: HashMap<String, Array<f64>>,
    str_arrays: HashMap<String, Array<String>>,
}

/// Parsed program plus its `DATA` pool, rebuilt whenever the program changes.
struct Compiled {
    lines: BTreeMap<LineNo, Result<Rc<[Stmt]>>>,
    data: Vec<(LineNo, DataItem)>,
}

impl Compiled {
    fn build(program: &Program) -> Self {
        let mut lines = BTreeMap::new();
        let mut data = Vec::new();
        for (n, src) in program.iter() {
            let parsed = parser::parse_line(src)
                .map_err(|e| BasicError::at(ErrorCode::Nonsense, n, e.statement));
            if let Ok(stmts) = &parsed {
                for stmt in stmts {
                    if let Stmt::Data(items) = stmt {
                        data.extend(items.iter().map(|item| (n, item.clone())));
                    }
                }
            }
            lines.insert(n, parsed.map(Rc::from));
        }
        Compiled { lines, data }
    }

    fn first_line(&self) -> Option<LineNo> {
        self.lines.keys().next().copied()
    }

    fn line_after(&self, n: LineNo) -> Option<LineNo> {
        self.lines
            .range((Bound::Excluded(n), Bound::Unbounded))
            .next()
            .map(|(&n, _)| n)
    }

    fn contains(&self, n: LineNo) -> bool {
        self.lines.contains_key(&n)
    }
}

/// How a run of statements ended.
enum End {
    /// Ran off the end; carries the last program position executed, if any.
    Finished(Option<Pos>),
    Exit(i32),
    Edit(String),
    Auto {
        start: LineNo,
        step: LineNo,
    },
}

/// Control flow after a single statement.
enum Flow {
    Next,
    Jump(Pos),
    SkipLine,
    Halt,
    Exit(i32),
    Edit(String),
    Auto { start: LineNo, step: LineNo },
}

/// Small xorshift64* generator for `RND`.
struct Rng(u64);

impl Rng {
    fn from_time() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545_f491_4f6c_dd1d);
        Rng(seed | 1)
    }

    fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let x = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub struct Interpreter {
    program: Program,
    compiled: Option<Rc<Compiled>>,
    program_changed: bool,
    vars: Variables,
    gosub: Vec<Pos>,
    fors: Vec<ForFrame>,
    data_cursor: usize,
    out: Output,
    err: Box<dyn Write>,
    console: Console,
    rng: Rng,
    interactive: bool,
    steps: u32,
}

impl Interpreter {
    pub fn new(out: Output, err: Box<dyn Write>, console: Console, interactive: bool) -> Self {
        Interpreter {
            program: Program::new(),
            compiled: None,
            program_changed: false,
            vars: Variables::default(),
            gosub: Vec::new(),
            fors: Vec::new(),
            data_cursor: 0,
            out,
            err,
            console,
            rng: Rng::from_time(),
            interactive,
            steps: 0,
        }
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Handles one line typed at the prompt (or read from a pipe).
    pub fn enter_line(&mut self, line: &str, source: &mut dyn LineSource) -> Outcome {
        let line = line.trim();
        if line.is_empty() {
            return Outcome::Continue;
        }
        if line.starts_with(|c: char| c.is_ascii_digit()) {
            if let Err(e) = self.store_line(line) {
                self.report(&e);
            }
            return Outcome::Continue;
        }

        let stmts: Rc<[Stmt]> = match parser::parse_line(line) {
            Ok(stmts) => Rc::from(stmts),
            Err(_) => {
                self.report(&BasicError::immediate(ErrorCode::Nonsense));
                return Outcome::Continue;
            }
        };

        // Loop and subroutine frames from an earlier immediate line are stale.
        self.fors.retain(|f| f.body.loc != Loc::Immediate);
        self.gosub.retain(|p| p.loc != Loc::Immediate);

        let start = Pos {
            loc: Loc::Immediate,
            stmt: 0,
        };
        let result = self.execute(&stmts, start, source);
        let outcome = match result {
            Ok(End::Finished(Some(pos))) => {
                if self.interactive {
                    let Loc::Line(n) = pos.loc else {
                        unreachable!("finished positions are program lines")
                    };
                    self.report(&BasicError::at(ErrorCode::Ok, n, stmt_number(pos)));
                }
                Outcome::Continue
            }
            Ok(End::Finished(None)) => Outcome::Continue,
            Ok(End::Exit(code)) => Outcome::Exit(code),
            Ok(End::Edit(text)) => Outcome::Edit(text),
            Ok(End::Auto { start, step }) => Outcome::Auto { start, step },
            Err(e) => {
                self.report(&e);
                Outcome::Continue
            }
        };
        self.out.flush();
        outcome
    }

    /// Stores, replaces or deletes a numbered line after canonicalizing and
    /// verifying it (SPECS-v2.md §1). The store is untouched on error.
    pub fn store_line(&mut self, line: &str) -> Result<()> {
        let line = line.trim();
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        let line_no = line[..digits]
            .parse::<u32>()
            .map_err(|_| BasicError::immediate(ErrorCode::IntegerOutOfRange))
            .and_then(Program::validate_line_no)?;
        let text = line[digits..].trim();
        if text.is_empty() {
            self.program.delete(line_no);
        } else {
            let nonsense = |statement| BasicError::at(ErrorCode::Nonsense, line_no, statement);
            let tokens = lexer::tokenize(text).map_err(|e| nonsense(e.statement))?;
            parser::parse_tokens(&tokens).map_err(|e| nonsense(e.statement))?;
            self.program
                .insert(line_no, lexer::canonicalize(text, &tokens));
        }
        self.invalidate();
        Ok(())
    }

    /// Prints a report on its own line.
    pub fn report(&mut self, e: &BasicError) {
        if self.out.column() != 0 {
            self.out.newline();
        }
        self.out.flush();
        let _ = writeln!(self.err, "{e}");
        let _ = self.err.flush();
    }

    fn invalidate(&mut self) {
        self.compiled = None;
        self.program_changed = true;
        self.data_cursor = 0;
    }

    fn compiled(&mut self) -> Rc<Compiled> {
        if let Some(compiled) = &self.compiled {
            return compiled.clone();
        }
        let compiled = Rc::new(Compiled::build(&self.program));
        self.compiled = Some(compiled.clone());
        compiled
    }

    /// Clears variables, stacks and the DATA pointer (`RUN`, `CLEAR`, `NEW`).
    fn reset_runtime(&mut self) {
        self.vars = Variables::default();
        self.gosub.clear();
        self.fors.clear();
        self.data_cursor = 0;
    }

    // ----- execution loop -----

    fn execute(
        &mut self,
        imm: &Rc<[Stmt]>,
        start: Pos,
        source: &mut dyn LineSource,
    ) -> Result<End> {
        self.program_changed = false;
        self.console.enter_run_mode();
        let result = self.run(imm, start, source);
        self.console.leave_run_mode();
        result
    }

    fn run(&mut self, imm: &Rc<[Stmt]>, start: Pos, source: &mut dyn LineSource) -> Result<End> {
        let mut prog = self.compiled();
        let mut pos = start;
        let mut current: Option<(Loc, Rc<[Stmt]>)> = None;
        let mut last_in_program = None;

        loop {
            let stmts = match &current {
                Some((loc, stmts)) if *loc == pos.loc => stmts.clone(),
                _ => {
                    let stmts = match pos.loc {
                        Loc::Immediate => imm.clone(),
                        Loc::Line(n) => match prog.lines.get(&n) {
                            Some(Ok(stmts)) => stmts.clone(),
                            Some(Err(e)) => return Err(e.clone()),
                            None => return Err(ErrorCode::StatementLost.into()),
                        },
                    };
                    current = Some((pos.loc, stmts.clone()));
                    stmts
                }
            };

            if pos.stmt >= stmts.len() {
                match pos.loc {
                    Loc::Immediate => return Ok(End::Finished(None)),
                    Loc::Line(n) => match prog.line_after(n) {
                        Some(next) => {
                            pos = Pos::line_start(next);
                            continue;
                        }
                        None => return Ok(End::Finished(last_in_program)),
                    },
                }
            }

            self.steps = self.steps.wrapping_add(1);
            if self.steps % BREAK_CHECK_INTERVAL == 0 {
                self.out.flush();
                if self.console.poll_break() {
                    return Err(locate(ErrorCode::Break.into(), pos));
                }
            }

            let flow = self
                .exec(&stmts[pos.stmt], pos, &stmts, &prog, source)
                .map_err(|e| locate(e, pos))?;
            if let Loc::Line(_) = pos.loc {
                last_in_program = Some(pos);
            }

            if self.program_changed {
                self.program_changed = false;
                // NEW or LOAD from inside a program ends the run.
                if let Loc::Line(_) = pos.loc {
                    return Ok(End::Finished(None));
                }
                prog = self.compiled();
                current = None;
            }

            pos = match flow {
                Flow::Next => pos.next(),
                Flow::Jump(target) => target,
                Flow::SkipLine => Pos {
                    loc: pos.loc,
                    stmt: stmts.len(),
                },
                Flow::Halt => return Ok(End::Finished(None)),
                Flow::Exit(code) => return Ok(End::Exit(code)),
                Flow::Edit(text) => return Ok(End::Edit(text)),
                Flow::Auto { start, step } => return Ok(End::Auto { start, step }),
            };
        }
    }

    fn exec(
        &mut self,
        stmt: &Stmt,
        pos: Pos,
        stmts: &[Stmt],
        prog: &Compiled,
        source: &mut dyn LineSource,
    ) -> Result<Flow> {
        match stmt {
            Stmt::Print(items) => self.print(items)?,
            Stmt::Let(target, expr) => {
                let value = self.eval(expr)?;
                self.assign(target, value)?;
            }
            Stmt::If(cond) => {
                if self.eval_num(cond)? == 0.0 {
                    return Ok(Flow::SkipLine);
                }
            }
            Stmt::Goto(expr) => {
                let line = self.jump_target(expr, prog)?;
                return Ok(Flow::Jump(Pos::line_start(line)));
            }
            Stmt::Gosub(expr) => {
                let line = self.jump_target(expr, prog)?;
                if self.gosub.len() >= MAX_GOSUB_DEPTH {
                    return Err(ErrorCode::OutOfMemory.into());
                }
                self.gosub.push(pos.next());
                return Ok(Flow::Jump(Pos::line_start(line)));
            }
            Stmt::Return => {
                let target = self.gosub.pop().ok_or(ErrorCode::ReturnWithoutGosub)?;
                check_exists(target, prog)?;
                return Ok(Flow::Jump(target));
            }
            Stmt::For {
                var,
                start,
                limit,
                step,
            } => return self.exec_for(var, start, limit, step.as_ref(), pos, stmts, prog),
            Stmt::Next(var) => return self.exec_next(var, prog),
            Stmt::Stop => return Err(ErrorCode::Stop.into()),
            Stmt::Input(items) => self.input(items, source)?,
            Stmt::Data(_) | Stmt::Rem => {}
            Stmt::Read(targets) => {
                for target in targets {
                    let (_, item) = prog
                        .data
                        .get(self.data_cursor)
                        .ok_or(ErrorCode::EndOfData)?;
                    self.data_cursor += 1;
                    let value = match (target.ty(), item) {
                        (Type::Str, DataItem::Quoted(s) | DataItem::Raw(s)) => {
                            Value::Str(s.clone())
                        }
                        (Type::Num, DataItem::Raw(s)) => Value::Number(self.eval_val(s)?),
                        (Type::Num, DataItem::Quoted(_)) => {
                            return Err(ErrorCode::Nonsense.into());
                        }
                    };
                    self.assign(target, value)?;
                }
            }
            Stmt::Restore(line) => {
                let from = match line {
                    Some(expr) => self.line_number(expr)?,
                    None => MIN_LINE,
                };
                self.data_cursor = prog.data.partition_point(|(n, _)| *n < from);
            }
            Stmt::Dim(array, dims) => {
                let mut sizes = Vec::with_capacity(dims.len());
                for dim in dims {
                    let n = self.eval_int(dim)?;
                    if n < 1 {
                        return Err(ErrorCode::IntegerOutOfRange.into());
                    }
                    sizes.push(usize::try_from(n).map_err(|_| ErrorCode::OutOfMemory)?);
                }
                let exists = match array.ty {
                    Type::Num => self.vars.num_arrays.contains_key(&array.name),
                    Type::Str => self.vars.str_arrays.contains_key(&array.name),
                };
                if exists {
                    return Err(ErrorCode::Nonsense.into());
                }
                match array.ty {
                    Type::Num => {
                        let arr = Array::new(sizes, 0.0)?;
                        self.vars.num_arrays.insert(array.name.clone(), arr);
                    }
                    Type::Str => {
                        let arr = Array::new(sizes, String::new())?;
                        self.vars.str_arrays.insert(array.name.clone(), arr);
                    }
                }
            }
            Stmt::Cls => self.out.cls(),
            Stmt::List(from) => {
                let from = match from {
                    Some(expr) => self.line_number(expr)?,
                    None => MIN_LINE,
                };
                let listing: Vec<String> = self
                    .program
                    .iter_from(from)
                    .map(|(n, text)| format!("{n} {text}\n"))
                    .collect();
                for line in listing {
                    self.out.write_str(&line);
                }
            }
            Stmt::Run(from) => {
                let target = match from {
                    Some(expr) => Some(self.jump_target(expr, prog)?),
                    None => prog.first_line(),
                };
                self.reset_runtime();
                return Ok(match target {
                    Some(line) => Flow::Jump(Pos::line_start(line)),
                    None => Flow::Halt,
                });
            }
            Stmt::New => {
                self.program.clear();
                self.reset_runtime();
                self.invalidate();
            }
            Stmt::Clear => self.reset_runtime(),
            Stmt::Save(path) => {
                let path = self.eval_str(path)?;
                self.save(&path)?;
            }
            Stmt::Load(path) => {
                let path = self.eval_str(path)?;
                self.load(&path)?;
            }
            Stmt::Exit(code) => {
                let code = match code {
                    Some(expr) => {
                        let n = self.eval_int(expr)?;
                        i32::try_from(n).map_err(|_| ErrorCode::IntegerOutOfRange)?
                    }
                    None => 0,
                };
                return Ok(Flow::Exit(code));
            }
            Stmt::Edit(line) => {
                if pos.loc != Loc::Immediate || !self.interactive {
                    return Err(ErrorCode::Nonsense.into());
                }
                let n = self.line_number(line)?;
                let text = self.program.get(n).ok_or(ErrorCode::IntegerOutOfRange)?;
                return Ok(Flow::Edit(format!("{n} {text}")));
            }
            Stmt::Auto(start, step) => {
                if pos.loc != Loc::Immediate {
                    return Err(ErrorCode::Nonsense.into());
                }
                let (start, step) = self.start_and_step(start.as_ref(), step.as_ref())?;
                return Ok(Flow::Auto { start, step });
            }
            Stmt::Renum(start, step) => {
                if pos.loc != Loc::Immediate {
                    return Err(ErrorCode::Nonsense.into());
                }
                let (start, step) = self.start_and_step(start.as_ref(), step.as_ref())?;
                let lines = renum::renumber(&self.program, start, step, &mut self.err)?;
                self.program.replace_all(lines);
                self.reset_runtime();
                self.invalidate();
            }
        }
        Ok(Flow::Next)
    }

    #[allow(clippy::too_many_arguments)]
    fn exec_for(
        &mut self,
        var: &str,
        start: &Expr,
        limit: &Expr,
        step: Option<&Expr>,
        pos: Pos,
        stmts: &[Stmt],
        prog: &Compiled,
    ) -> Result<Flow> {
        let start = self.eval_num(start)?;
        let limit = self.eval_num(limit)?;
        let step = match step {
            Some(expr) => self.eval_num(expr)?,
            None => 1.0,
        };
        self.vars.nums.insert(var.to_string(), start);
        self.fors.retain(|f| f.var != var);

        let runs = if step >= 0.0 {
            start <= limit
        } else {
            start >= limit
        };
        if !runs {
            // Skip the body: continue after the matching NEXT.
            let after = find_next(var, pos, stmts, prog).ok_or(ErrorCode::NextWithoutFor)?;
            return Ok(Flow::Jump(after));
        }
        self.fors.push(ForFrame {
            var: var.to_string(),
            limit,
            step,
            body: pos.next(),
        });
        Ok(Flow::Next)
    }

    fn exec_next(&mut self, var: &str, prog: &Compiled) -> Result<Flow> {
        let index = self
            .fors
            .iter()
            .rposition(|f| f.var == var)
            .ok_or(ErrorCode::NextWithoutFor)?;
        self.fors.truncate(index + 1);
        let frame = &self.fors[index];
        let value = self.vars.nums.get(var).ok_or(ErrorCode::VariableNotFound)? + frame.step;
        let again = if frame.step >= 0.0 {
            value <= frame.limit
        } else {
            value >= frame.limit
        };
        let body = frame.body;
        self.vars.nums.insert(var.to_string(), check_num(value)?);
        if again {
            check_exists(body, prog)?;
            Ok(Flow::Jump(body))
        } else {
            self.fors.pop();
            Ok(Flow::Next)
        }
    }

    // ----- statements with I/O -----

    fn print(&mut self, items: &[PrintItem]) -> Result<()> {
        let mut newline = true;
        for item in items {
            match item {
                PrintItem::Expr(expr) => {
                    let text = match self.eval(expr)? {
                        Value::Number(n) => format_number(n),
                        Value::Str(s) => s,
                    };
                    self.out.write_str(&text);
                    newline = true;
                }
                PrintItem::Semicolon => newline = false,
                PrintItem::Comma => {
                    self.out.tab();
                    newline = false;
                }
                PrintItem::Newline => {
                    self.out.newline();
                    newline = true;
                }
            }
        }
        if newline {
            self.out.newline();
        }
        Ok(())
    }

    fn input(&mut self, items: &[InputItem], source: &mut dyn LineSource) -> Result<()> {
        let mut prompt = String::new();
        for item in items {
            match item {
                InputItem::Prompt(text) => prompt.push_str(text),
                InputItem::Semicolon => {}
                InputItem::Comma => {
                    let column = prompt
                        .rsplit('\n')
                        .next()
                        .map_or(0, |last| last.chars().count());
                    prompt.push_str(&" ".repeat(TAB_WIDTH - column % TAB_WIDTH));
                }
                InputItem::Newline => prompt.push('\n'),
                InputItem::Var(target) => {
                    let value = self.read_input(&prompt, target.ty(), source)?;
                    self.assign(target, value)?;
                    prompt.clear();
                }
            }
        }
        Ok(())
    }

    fn read_input(&mut self, prompt: &str, ty: Type, source: &mut dyn LineSource) -> Result<Value> {
        let (head, last) = match prompt.rsplit_once('\n') {
            Some((head, last)) => (Some(head), last),
            None => (None, prompt),
        };
        if let Some(head) = head {
            self.out.write_str(head);
            self.out.newline();
        }
        loop {
            let line = if self.interactive {
                self.out.flush();
                self.console.leave_run_mode();
                let line = source.read_line(last, "");
                self.console.enter_run_mode();
                self.out.column = 0;
                line
            } else {
                self.out.write_str(last);
                self.out.flush();
                source.read_line("", "")
            };
            let text = match line {
                Ok(Input::Line(text)) => text,
                Ok(Input::Interrupted) => return Err(ErrorCode::Break.into()),
                Ok(Input::Eof) => return Err(ErrorCode::StopInInput.into()),
                Err(_) => return Err(ErrorCode::Nonsense.into()),
            };
            match ty {
                Type::Str => return Ok(Value::Str(text)),
                Type::Num => match self.eval_val(&text) {
                    Ok(n) => return Ok(Value::Number(n)),
                    // Let the user try again; a script has no one to retry.
                    Err(_) if self.interactive => continue,
                    Err(e) => return Err(e),
                },
            }
        }
    }

    fn save(&mut self, path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(ErrorCode::FileError.into());
        }
        let mut text = String::new();
        for (n, line) in self.program.iter() {
            text.push_str(&format!("{n} {line}\n"));
        }
        fs::write(path, text).map_err(|e| BasicError::from_io(&e))
    }

    fn load(&mut self, path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(ErrorCode::FileError.into());
        }
        // Read the whole file first so a missing file leaves the program intact.
        let bytes = fs::read(path).map_err(|e| BasicError::from_io(&e))?;
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::FileError)?;
        self.program.clear();
        self.reset_runtime();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let stored = if line.starts_with(|c: char| c.is_ascii_digit()) {
                self.store_line(line)
            } else {
                Err(ErrorCode::Nonsense.into())
            };
            if let Err(e) = stored {
                self.report(&e);
            }
        }
        self.invalidate();
        Ok(())
    }

    // ----- assignment -----

    fn assign(&mut self, target: &LValue, value: Value) -> Result<()> {
        match target {
            LValue::Num(name, None) => {
                self.vars.nums.insert(name.clone(), value.num()?);
            }
            LValue::Num(name, Some(subs)) => {
                let value = value.num()?;
                let index = self.eval_indices(subs)?;
                let array = self
                    .vars
                    .num_arrays
                    .get_mut(name)
                    .ok_or(ErrorCode::VariableNotFound)?;
                let offset = array.offset(&index)?;
                array.data[offset] = value;
            }
            LValue::Str(name, None) => {
                let value = check_len(value.string()?)?;
                self.vars.strs.insert(name.clone(), value);
            }
            LValue::Str(name, Some(Subscript::Index(subs)))
                if self.vars.str_arrays.contains_key(name) =>
            {
                let value = check_len(value.string()?)?;
                let index = self.eval_indices(subs)?;
                let array = self
                    .vars
                    .str_arrays
                    .get_mut(name)
                    .ok_or(ErrorCode::VariableNotFound)?;
                let offset = array.offset(&index)?;
                array.data[offset] = value;
            }
            LValue::Str(name, Some(sub)) => {
                // Assignment to a substring replaces it in place, padding or
                // truncating the new text to the slice length.
                let value = value.string()?;
                let current = self
                    .vars
                    .strs
                    .get(name)
                    .ok_or(ErrorCode::VariableNotFound)?
                    .clone();
                let mut chars: Vec<char> = current.chars().collect();
                if let Some((from, to)) = self.slice_bounds(sub, chars.len())? {
                    let mut replacement = value.chars().chain(std::iter::repeat(' '));
                    for c in &mut chars[from..to] {
                        *c = replacement.next().unwrap_or(' ');
                    }
                }
                self.vars
                    .strs
                    .insert(name.clone(), chars.into_iter().collect());
            }
        }
        Ok(())
    }

    // ----- expressions -----

    fn eval_num(&mut self, expr: &Expr) -> Result<f64> {
        self.eval(expr)?.num()
    }

    fn eval_str(&mut self, expr: &Expr) -> Result<String> {
        self.eval(expr)?.string()
    }

    /// Integer conversion for subscripts, dimensions and codes: truncates
    /// toward zero (BASIC-SPECS.md §2.1).
    fn eval_int(&mut self, expr: &Expr) -> Result<i64> {
        to_int(self.eval_num(expr)?.trunc())
    }

    /// A line number in 1..=9999, rounded (BASIC-SPECS.md §4.3).
    fn line_number(&mut self, expr: &Expr) -> Result<LineNo> {
        let n = to_int(self.eval_num(expr)?.round())?;
        u32::try_from(n)
            .map_err(|_| ErrorCode::IntegerOutOfRange.into())
            .and_then(Program::validate_line_no)
    }

    /// A line number that must exist in the program (`GOTO`, `GOSUB`, `RUN n`).
    fn jump_target(&mut self, expr: &Expr, prog: &Compiled) -> Result<LineNo> {
        let line = self.line_number(expr)?;
        if !prog.contains(line) {
            return Err(ErrorCode::StatementLost.into());
        }
        Ok(line)
    }

    fn start_and_step(
        &mut self,
        start: Option<&Expr>,
        step: Option<&Expr>,
    ) -> Result<(LineNo, LineNo)> {
        let start = match start {
            Some(expr) => self.line_number(expr)?,
            None => 10,
        };
        let step = match step {
            Some(expr) => self.line_number(expr)?,
            None => 10,
        };
        Ok((start, step))
    }

    /// Evaluates text as a numeric expression (`VAL`, `INPUT`, `READ`).
    fn eval_val(&mut self, text: &str) -> Result<f64> {
        let expr = parser::parse_numeric_expr(text).ok_or(ErrorCode::Nonsense)?;
        self.eval_num(&expr)
    }

    fn eval_indices(&mut self, subs: &[Expr]) -> Result<Vec<i64>> {
        subs.iter().map(|e| self.eval_int(e)).collect()
    }

    /// 0-based half-open character range for a slice, or `None` when empty.
    fn slice_bounds(&mut self, sub: &Subscript, len: usize) -> Result<Option<(usize, usize)>> {
        let (from, to) = match sub {
            Subscript::Index(args) => {
                let [arg] = args.as_slice() else {
                    return Err(ErrorCode::SubscriptOutOfRange.into());
                };
                let i = self.eval_int(arg)?;
                (i, i)
            }
            Subscript::Range(from, to) => {
                let from = match from {
                    Some(e) => self.eval_int(e)?,
                    None => 1,
                };
                let to = match to {
                    Some(e) => self.eval_int(e)?,
                    None => len as i64,
                };
                (from, to)
            }
        };
        if from > to {
            return Ok(None);
        }
        if from < 1 || to > len as i64 {
            return Err(ErrorCode::SubscriptOutOfRange.into());
        }
        Ok(Some((from as usize - 1, to as usize)))
    }

    fn slice(&mut self, s: &str, sub: &Subscript) -> Result<String> {
        let chars: Vec<char> = s.chars().collect();
        Ok(match self.slice_bounds(sub, chars.len())? {
            Some((from, to)) => chars[from..to].iter().collect(),
            None => String::new(),
        })
    }

    fn eval(&mut self, expr: &Expr) -> Result<Value> {
        Ok(match expr {
            Expr::Num(n) => Value::Number(*n),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::NumVar(name, None) => Value::Number(
                *self
                    .vars
                    .nums
                    .get(name)
                    .ok_or(ErrorCode::VariableNotFound)?,
            ),
            Expr::NumVar(name, Some(subs)) => {
                let index = self.eval_indices(subs)?;
                let array = self
                    .vars
                    .num_arrays
                    .get(name)
                    .ok_or(ErrorCode::VariableNotFound)?;
                Value::Number(array.data[array.offset(&index)?])
            }
            Expr::StrVar(name, None) => Value::Str(
                self.vars
                    .strs
                    .get(name)
                    .ok_or(ErrorCode::VariableNotFound)?
                    .clone(),
            ),
            Expr::StrVar(name, Some(Subscript::Index(subs)))
                if self.vars.str_arrays.contains_key(name) =>
            {
                let index = self.eval_indices(subs)?;
                let array = self
                    .vars
                    .str_arrays
                    .get(name)
                    .ok_or(ErrorCode::VariableNotFound)?;
                Value::Str(array.data[array.offset(&index)?].clone())
            }
            Expr::StrVar(name, Some(sub)) => {
                let s = self
                    .vars
                    .strs
                    .get(name)
                    .ok_or(ErrorCode::VariableNotFound)?
                    .clone();
                Value::Str(self.slice(&s, sub)?)
            }
            Expr::Slice(inner, sub) => {
                let s = self.eval_str(inner)?;
                Value::Str(self.slice(&s, sub)?)
            }
            Expr::Unary(UnOp::Neg, operand) => Value::Number(-self.eval_num(operand)?),
            Expr::Unary(UnOp::Not, operand) => Value::Number(truth(self.eval_num(operand)? == 0.0)),
            Expr::Binary(op, left, right) => self.binary(*op, left, right)?,
            Expr::Func(kw, arg) => self.function(*kw, arg.as_deref())?,
        })
    }

    fn binary(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Result<Value> {
        let l = self.eval(left)?;
        let r = self.eval(right)?;
        let num = |x: f64| check_num(x).map(Value::Number);
        match (op, l, r) {
            (BinOp::Add, Value::Str(a), Value::Str(b)) => Ok(Value::Str(check_len(a + &b)?)),
            (BinOp::Add, Value::Number(a), Value::Number(b)) => num(a + b),
            (BinOp::Sub, Value::Number(a), Value::Number(b)) => num(a - b),
            (BinOp::Mul, Value::Number(a), Value::Number(b)) => num(a * b),
            (BinOp::Div, Value::Number(a), Value::Number(b)) => {
                if b == 0.0 {
                    Err(ErrorCode::NumberTooBig.into())
                } else {
                    num(a / b)
                }
            }
            (BinOp::Pow, Value::Number(a), Value::Number(b)) => num(a.powf(b)),
            // AND: left operand if the right one is true, else 0 (or "").
            (BinOp::And, Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number(if b != 0.0 { a } else { 0.0 }))
            }
            (BinOp::And, Value::Str(a), Value::Number(b)) => {
                Ok(Value::Str(if b != 0.0 { a } else { String::new() }))
            }
            // OR: left operand if it is true, else the right one.
            (BinOp::Or, Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number(if a != 0.0 { a } else { b }))
            }
            (op, Value::Number(a), Value::Number(b)) => compare(op, a.partial_cmp(&b)),
            (op, Value::Str(a), Value::Str(b)) => compare(op, Some(a.cmp(&b))),
            _ => Err(ErrorCode::Nonsense.into()),
        }
    }

    fn function(&mut self, kw: Keyword, arg: Option<&Expr>) -> Result<Value> {
        match kw {
            Keyword::Rnd => return Ok(Value::Number(self.rng.next_f64())),
            Keyword::InkeyS => {
                let key = self
                    .console
                    .inkey()
                    .map_err(|Break| BasicError::from(ErrorCode::Break))?;
                return Ok(Value::Str(key.map(String::from).unwrap_or_default()));
            }
            _ => {}
        }
        let arg = arg.ok_or(ErrorCode::Nonsense)?;
        let invalid = || BasicError::from(ErrorCode::InvalidArgument);
        let value = match kw {
            Keyword::Len => return Ok(Value::Number(self.eval_str(arg)?.chars().count() as f64)),
            Keyword::Code => {
                let s = self.eval_str(arg)?;
                return Ok(Value::Number(
                    s.chars().next().map_or(0.0, |c| c as u32 as f64),
                ));
            }
            Keyword::Val => {
                let s = self.eval_str(arg)?;
                return Ok(Value::Number(self.eval_val(&s)?));
            }
            Keyword::StrS => return Ok(Value::Str(format_number(self.eval_num(arg)?))),
            Keyword::ChrS => {
                let n = self.eval_int(arg)?;
                let byte = u8::try_from(n).map_err(|_| ErrorCode::IntegerOutOfRange)?;
                return Ok(Value::Str(char::from(byte).to_string()));
            }
            _ => self.eval_num(arg)?,
        };
        let result = match kw {
            Keyword::Abs => value.abs(),
            Keyword::Acs | Keyword::Asn if !(-1.0..=1.0).contains(&value) => return Err(invalid()),
            Keyword::Acs => value.acos(),
            Keyword::Asn => value.asin(),
            Keyword::Atn => value.atan(),
            Keyword::Cos => value.cos(),
            Keyword::Exp => value.exp(),
            Keyword::Int => value.floor(),
            Keyword::Ln if value <= 0.0 => return Err(invalid()),
            Keyword::Ln => value.ln(),
            Keyword::Sgn => {
                if value > 0.0 {
                    1.0
                } else if value < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            Keyword::Sin => value.sin(),
            Keyword::Sqr if value < 0.0 => return Err(invalid()),
            Keyword::Sqr => value.sqrt(),
            Keyword::Tan => value.tan(),
            _ => return Err(ErrorCode::Nonsense.into()),
        };
        check_num(result).map(Value::Number)
    }
}

/// Attaches the program position to an error raised while executing it.
fn locate(e: BasicError, pos: Pos) -> BasicError {
    match (e.line, pos.loc) {
        (None, Loc::Line(n)) => BasicError::at(e.code, n, stmt_number(pos)),
        _ => e,
    }
}

fn stmt_number(pos: Pos) -> u16 {
    u16::try_from(pos.stmt + 1).unwrap_or(u16::MAX)
}

fn check_exists(pos: Pos, prog: &Compiled) -> Result<()> {
    match pos.loc {
        Loc::Line(n) if !prog.contains(n) => Err(ErrorCode::StatementLost.into()),
        _ => Ok(()),
    }
}

/// Finds the statement after the `NEXT var` that closes a skipped loop.
fn find_next(var: &str, pos: Pos, stmts: &[Stmt], prog: &Compiled) -> Option<Pos> {
    let is_next = |s: &Stmt| matches!(s, Stmt::Next(v) if v == var);
    if let Some(i) = stmts[pos.stmt + 1..].iter().position(is_next) {
        return Some(Pos {
            loc: pos.loc,
            stmt: pos.stmt + 1 + i + 1,
        });
    }
    let Loc::Line(line) = pos.loc else {
        return None;
    };
    prog.lines
        .range((Bound::Excluded(line), Bound::Unbounded))
        .find_map(|(&n, stmts)| {
            let stmts = stmts.as_ref().ok()?;
            let i = stmts.iter().position(is_next)?;
            Some(Pos {
                loc: Loc::Line(n),
                stmt: i + 1,
            })
        })
}

fn truth(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

fn compare(op: BinOp, ord: Option<std::cmp::Ordering>) -> Result<Value> {
    use std::cmp::Ordering::{Equal, Greater, Less};
    let result = match (op, ord) {
        (BinOp::Eq, o) => o == Some(Equal),
        (BinOp::Ne, o) => o != Some(Equal),
        (BinOp::Lt, o) => o == Some(Less),
        (BinOp::Gt, o) => o == Some(Greater),
        (BinOp::Le, o) => matches!(o, Some(Less | Equal)),
        (BinOp::Ge, o) => matches!(o, Some(Greater | Equal)),
        _ => return Err(ErrorCode::Nonsense.into()),
    };
    Ok(Value::Number(truth(result)))
}

/// Rejects results that are not finite numbers.
fn check_num(x: f64) -> Result<f64> {
    if x.is_nan() {
        Err(ErrorCode::InvalidArgument.into())
    } else if x.is_infinite() {
        Err(ErrorCode::NumberTooBig.into())
    } else {
        Ok(x)
    }
}

fn check_len(s: String) -> Result<String> {
    if s.len() > MAX_STRING_LEN && s.chars().count() > MAX_STRING_LEN {
        Err(ErrorCode::OutOfMemory.into())
    } else {
        Ok(s)
    }
}

fn to_int(x: f64) -> Result<i64> {
    if x.is_finite() && x.abs() < 9e15 {
        Ok(x as i64)
    } else {
        Err(ErrorCode::IntegerOutOfRange.into())
    }
}
