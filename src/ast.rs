//! Syntax tree for parsed BASIC statements.

use crate::lexer::Keyword;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Num,
    Str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

/// `( ... )` after a string: an index list or a `TO` range.
#[derive(Debug, Clone, PartialEq)]
pub enum Subscript {
    /// `(i)` or `(i, j, ...)`: array element, or a single character when
    /// there is no string array of that name.
    Index(Vec<Expr>),
    /// `(s TO e)`, `(s TO)`, `(TO e)`.
    Range(Option<Box<Expr>>, Option<Box<Expr>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(String),
    /// Numeric variable, or numeric array element when subscripted.
    NumVar(String, Option<Vec<Expr>>),
    /// String variable (`A$`), optionally subscripted.
    StrVar(String, Option<Subscript>),
    /// Slice of an arbitrary string expression, e.g. `"hello"(2 TO 3)`.
    Slice(Box<Expr>, Subscript),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Built-in function; `None` for argument-less functions (`RND`, `INKEY$`).
    Func(Keyword, Option<Box<Expr>>),
}

/// Assignment target of `LET`, `INPUT`, `READ` and `FOR`.
#[derive(Debug, Clone, PartialEq)]
pub enum LValue {
    Num(String, Option<Vec<Expr>>),
    Str(String, Option<Subscript>),
}

impl LValue {
    pub fn ty(&self) -> Type {
        match self {
            LValue::Num(..) => Type::Num,
            LValue::Str(..) => Type::Str,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrintItem {
    Expr(Expr),
    /// `;`
    Semicolon,
    /// `,`
    Comma,
    /// `'`
    Newline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputItem {
    Prompt(String),
    Var(LValue),
    Semicolon,
    Comma,
    Newline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataItem {
    /// Quoted string literal (already unescaped).
    Quoted(String),
    /// Unquoted text, trimmed.
    Raw(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Print(Vec<PrintItem>),
    Let(LValue, Expr),
    /// `IF cond THEN`; the statements after `THEN` follow as ordinary
    /// statements of the same line and are skipped when `cond` is false.
    If(Expr),
    Goto(Expr),
    Gosub(Expr),
    Return,
    For {
        var: String,
        start: Expr,
        limit: Expr,
        step: Option<Expr>,
    },
    Next(String),
    Stop,
    Input(Vec<InputItem>),
    Data(Vec<DataItem>),
    Read(Vec<LValue>),
    Restore(Option<Expr>),
    Dim(ArrayName, Vec<Expr>),
    Rem,
    Cls,
    List(Option<Expr>),
    Run(Option<Expr>),
    New,
    Clear,
    Save(Expr),
    Load(Expr),
    Exit(Option<Expr>),
    Edit(Expr),
    Auto(Option<Expr>, Option<Expr>),
    Renum(Option<Expr>, Option<Expr>),
}

/// Name of an array in `DIM`, tagged with its element type.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayName {
    pub name: String,
    pub ty: Type,
}
