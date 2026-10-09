//! Statement and expression parser with static type checking
//! (BASIC-SPECS.md §3–§5).
//!
//! Every numbered line is parsed when it is entered, so syntax and type
//! errors are reported as `C Nonsense in BASIC` before the line is stored.

use crate::ast::{
    ArrayName, BinOp, DataItem, Expr, InputItem, LValue, PrintItem, Stmt, Subscript, Type, UnOp,
};
use crate::lexer::{self, Keyword, Token, TokenKind};

/// A syntax error, located by its 1-based statement index within the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    pub statement: u16,
}

type PResult<T> = Result<T, ParseError>;

/// Parses one line of source (without its line number) into statements.
pub fn parse_line(src: &str) -> PResult<Vec<Stmt>> {
    let tokens = lexer::tokenize(src).map_err(|e| ParseError {
        statement: e.statement,
    })?;
    parse_tokens(&tokens)
}

/// Parses an already tokenized line.
pub fn parse_tokens(tokens: &[Token]) -> PResult<Vec<Stmt>> {
    Parser::new(tokens).line()
}

/// Parses a complete numeric expression (used by `VAL`, `INPUT` and `READ`).
pub fn parse_numeric_expr(src: &str) -> Option<Expr> {
    let tokens = lexer::tokenize(src).ok()?;
    let mut p = Parser::new(&tokens);
    let (expr, ty) = p.expr().ok()?;
    (ty == Type::Num && p.at_end()).then_some(expr)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    statement: u16,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Parser {
            tokens,
            pos: 0,
            statement: 1,
        }
    }

    fn error<T>(&self) -> PResult<T> {
        Err(ParseError {
            statement: self.statement,
        })
    }

    fn peek(&self) -> Option<&'a TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn advance(&mut self) -> Option<&'a TokenKind> {
        let kind = self.peek();
        if kind.is_some() {
            self.pos += 1;
        }
        kind
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// True at the end of a statement (`:` or end of line).
    fn at_statement_end(&self) -> bool {
        matches!(self.peek(), None | Some(TokenKind::Colon))
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_keyword(&mut self, kw: Keyword) -> bool {
        self.eat(&TokenKind::Keyword(kw))
    }

    fn expect(&mut self, kind: &TokenKind) -> PResult<()> {
        if self.eat(kind) { Ok(()) } else { self.error() }
    }

    // ----- statements -----

    fn line(&mut self) -> PResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        if self.at_end() {
            return Ok(stmts);
        }
        loop {
            let stmt = self.statement()?;
            let is_if = matches!(stmt, Stmt::If(_));
            let is_rem = matches!(stmt, Stmt::Rem);
            stmts.push(stmt);
            if is_rem {
                break;
            }
            if is_if {
                // The statement after THEN starts without a colon.
                self.statement += 1;
                continue;
            }
            match self.advance() {
                None => break,
                Some(TokenKind::Colon) => self.statement += 1,
                Some(_) => return self.error(),
            }
        }
        Ok(stmts)
    }

    fn statement(&mut self) -> PResult<Stmt> {
        let Some(TokenKind::Keyword(kw)) = self.advance() else {
            return self.error();
        };
        let stmt = match kw {
            Keyword::Print => Stmt::Print(self.print_items()?),
            Keyword::Let => {
                let target = self.lvalue()?;
                self.expect(&TokenKind::Eq)?;
                let value = self.typed_expr(target.ty())?;
                Stmt::Let(target, value)
            }
            Keyword::If => {
                let cond = self.typed_expr(Type::Num)?;
                if !self.eat_keyword(Keyword::Then) || self.at_statement_end() {
                    return self.error();
                }
                Stmt::If(cond)
            }
            Keyword::Goto => Stmt::Goto(self.typed_expr(Type::Num)?),
            Keyword::Gosub => Stmt::Gosub(self.typed_expr(Type::Num)?),
            Keyword::Return => Stmt::Return,
            Keyword::For => {
                let var = self.numeric_name()?;
                self.expect(&TokenKind::Eq)?;
                let start = self.typed_expr(Type::Num)?;
                if !self.eat_keyword(Keyword::To) {
                    return self.error();
                }
                let limit = self.typed_expr(Type::Num)?;
                let step = if self.eat_keyword(Keyword::Step) {
                    Some(self.typed_expr(Type::Num)?)
                } else {
                    None
                };
                Stmt::For {
                    var,
                    start,
                    limit,
                    step,
                }
            }
            Keyword::Next => Stmt::Next(self.numeric_name()?),
            Keyword::Stop => Stmt::Stop,
            Keyword::Input => Stmt::Input(self.input_items()?),
            Keyword::Data => match self.advance() {
                Some(TokenKind::Data(raw)) => Stmt::Data(self.data_items(raw)?),
                _ => return self.error(),
            },
            Keyword::Read => {
                let mut targets = vec![self.lvalue()?];
                while self.eat(&TokenKind::Comma) {
                    targets.push(self.lvalue()?);
                }
                Stmt::Read(targets)
            }
            Keyword::Restore => Stmt::Restore(self.optional_num()?),
            Keyword::Dim => {
                let Some(TokenKind::Ident(name)) = self.advance() else {
                    return self.error();
                };
                let ty = type_of_name(name);
                self.expect(&TokenKind::LParen)?;
                let dims = self.num_args()?;
                Stmt::Dim(
                    ArrayName {
                        name: name.clone(),
                        ty,
                    },
                    dims,
                )
            }
            Keyword::Rem => Stmt::Rem,
            Keyword::Cls => Stmt::Cls,
            Keyword::List => Stmt::List(self.optional_num()?),
            Keyword::Run => Stmt::Run(self.optional_num()?),
            Keyword::New => Stmt::New,
            Keyword::Clear => Stmt::Clear,
            Keyword::Save => Stmt::Save(self.typed_expr(Type::Str)?),
            Keyword::Load => Stmt::Load(self.typed_expr(Type::Str)?),
            Keyword::Exit => Stmt::Exit(self.optional_num()?),
            Keyword::Edit => Stmt::Edit(self.typed_expr(Type::Num)?),
            Keyword::Auto => {
                let (a, b) = self.optional_pair()?;
                Stmt::Auto(a, b)
            }
            Keyword::Renum => {
                let (a, b) = self.optional_pair()?;
                Stmt::Renum(a, b)
            }
            _ => return self.error(),
        };
        Ok(stmt)
    }

    fn optional_num(&mut self) -> PResult<Option<Expr>> {
        if self.at_statement_end() {
            Ok(None)
        } else {
            Ok(Some(self.typed_expr(Type::Num)?))
        }
    }

    fn optional_pair(&mut self) -> PResult<(Option<Expr>, Option<Expr>)> {
        let first = self.optional_num()?;
        let second = if first.is_some() && self.eat(&TokenKind::Comma) {
            Some(self.typed_expr(Type::Num)?)
        } else {
            None
        };
        Ok((first, second))
    }

    fn print_items(&mut self) -> PResult<Vec<PrintItem>> {
        let mut items = Vec::new();
        let mut after_expr = false;
        while !self.at_statement_end() {
            let item = match self.peek() {
                Some(TokenKind::Semicolon) => PrintItem::Semicolon,
                Some(TokenKind::Comma) => PrintItem::Comma,
                Some(TokenKind::Apostrophe) => PrintItem::Newline,
                _ => {
                    // Two expressions in a row need a separator between them.
                    if after_expr {
                        return self.error();
                    }
                    after_expr = true;
                    items.push(PrintItem::Expr(self.expr()?.0));
                    continue;
                }
            };
            self.pos += 1;
            after_expr = false;
            items.push(item);
        }
        Ok(items)
    }

    fn input_items(&mut self) -> PResult<Vec<InputItem>> {
        let mut items = Vec::new();
        while !self.at_statement_end() {
            let item = match self.peek() {
                Some(TokenKind::Semicolon) => InputItem::Semicolon,
                Some(TokenKind::Comma) => InputItem::Comma,
                Some(TokenKind::Apostrophe) => InputItem::Newline,
                Some(TokenKind::Str(s)) => InputItem::Prompt(s.clone()),
                Some(TokenKind::Ident(_)) => {
                    items.push(InputItem::Var(self.lvalue()?));
                    continue;
                }
                _ => return self.error(),
            };
            self.pos += 1;
            items.push(item);
        }
        if !items.iter().any(|i| matches!(i, InputItem::Var(_))) {
            return self.error();
        }
        Ok(items)
    }

    fn data_items(&self, raw: &str) -> PResult<Vec<DataItem>> {
        let mut items = Vec::new();
        for part in split_data(raw) {
            let part = part.trim();
            if let Some(inner) = part.strip_prefix('"') {
                let Some(body) = inner.strip_suffix('"') else {
                    return self.error();
                };
                if body.replace("\"\"", "").contains('"') {
                    return self.error();
                }
                items.push(DataItem::Quoted(body.replace("\"\"", "\"")));
            } else {
                items.push(DataItem::Raw(part.to_string()));
            }
        }
        if items.len() == 1 && items[0] == DataItem::Raw(String::new()) {
            return self.error();
        }
        Ok(items)
    }

    fn numeric_name(&mut self) -> PResult<String> {
        match self.advance() {
            Some(TokenKind::Ident(name)) if type_of_name(name) == Type::Num => Ok(name.clone()),
            _ => self.error(),
        }
    }

    fn lvalue(&mut self) -> PResult<LValue> {
        let Some(TokenKind::Ident(name)) = self.advance() else {
            return self.error();
        };
        let name = name.clone();
        match type_of_name(&name) {
            Type::Num => {
                let subs = if self.eat(&TokenKind::LParen) {
                    Some(self.num_args()?)
                } else {
                    None
                };
                Ok(LValue::Num(name, subs))
            }
            Type::Str => {
                let sub = if self.eat(&TokenKind::LParen) {
                    Some(self.subscript()?)
                } else {
                    None
                };
                Ok(LValue::Str(name, sub))
            }
        }
    }

    /// Comma-separated numeric expressions after `(`, consuming the `)`.
    fn num_args(&mut self) -> PResult<Vec<Expr>> {
        let mut args = vec![self.typed_expr(Type::Num)?];
        while self.eat(&TokenKind::Comma) {
            args.push(self.typed_expr(Type::Num)?);
        }
        self.expect(&TokenKind::RParen)?;
        Ok(args)
    }

    /// A string subscript after `(`: `i`, `i, j, ...` or a `TO` range.
    fn subscript(&mut self) -> PResult<Subscript> {
        if self.eat_keyword(Keyword::To) {
            let end = self.range_end()?;
            return Ok(Subscript::Range(None, end));
        }
        let first = self.typed_expr(Type::Num)?;
        if self.eat_keyword(Keyword::To) {
            let end = self.range_end()?;
            return Ok(Subscript::Range(Some(Box::new(first)), end));
        }
        let mut args = vec![first];
        while self.eat(&TokenKind::Comma) {
            args.push(self.typed_expr(Type::Num)?);
        }
        self.expect(&TokenKind::RParen)?;
        Ok(Subscript::Index(args))
    }

    /// Optional end of a `TO` range, consuming the `)`.
    fn range_end(&mut self) -> PResult<Option<Box<Expr>>> {
        if self.eat(&TokenKind::RParen) {
            return Ok(None);
        }
        let end = self.typed_expr(Type::Num)?;
        self.expect(&TokenKind::RParen)?;
        Ok(Some(Box::new(end)))
    }

    // ----- expressions (lowest to highest precedence) -----

    fn typed_expr(&mut self, want: Type) -> PResult<Expr> {
        let (expr, ty) = self.expr()?;
        if ty == want { Ok(expr) } else { self.error() }
    }

    fn expr(&mut self) -> PResult<(Expr, Type)> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> PResult<(Expr, Type)> {
        let (mut left, ty) = self.and_expr()?;
        while self.eat_keyword(Keyword::Or) {
            let (right, rty) = self.and_expr()?;
            if ty != Type::Num || rty != Type::Num {
                return self.error();
            }
            left = Expr::Binary(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok((left, ty))
    }

    fn and_expr(&mut self) -> PResult<(Expr, Type)> {
        let (mut left, ty) = self.not_expr()?;
        while self.eat_keyword(Keyword::And) {
            let (right, rty) = self.not_expr()?;
            if rty != Type::Num {
                return self.error();
            }
            left = Expr::Binary(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok((left, ty))
    }

    fn not_expr(&mut self) -> PResult<(Expr, Type)> {
        if self.eat_keyword(Keyword::Not) {
            let (operand, ty) = self.not_expr()?;
            if ty != Type::Num {
                return self.error();
            }
            return Ok((Expr::Unary(UnOp::Not, Box::new(operand)), Type::Num));
        }
        self.relational()
    }

    fn relational(&mut self) -> PResult<(Expr, Type)> {
        let (mut left, mut ty) = self.additive()?;
        loop {
            let op = match self.peek() {
                Some(TokenKind::Eq) => BinOp::Eq,
                Some(TokenKind::Ne) => BinOp::Ne,
                Some(TokenKind::Lt) => BinOp::Lt,
                Some(TokenKind::Gt) => BinOp::Gt,
                Some(TokenKind::Le) => BinOp::Le,
                Some(TokenKind::Ge) => BinOp::Ge,
                _ => return Ok((left, ty)),
            };
            self.pos += 1;
            let (right, rty) = self.additive()?;
            if ty != rty {
                return self.error();
            }
            left = Expr::Binary(op, Box::new(left), Box::new(right));
            ty = Type::Num;
        }
    }

    fn additive(&mut self) -> PResult<(Expr, Type)> {
        let (mut left, ty) = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(TokenKind::Plus) => BinOp::Add,
                Some(TokenKind::Minus) => BinOp::Sub,
                _ => return Ok((left, ty)),
            };
            self.pos += 1;
            let (right, rty) = self.multiplicative()?;
            let ok = match op {
                BinOp::Add => ty == rty,
                _ => ty == Type::Num && rty == Type::Num,
            };
            if !ok {
                return self.error();
            }
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn multiplicative(&mut self) -> PResult<(Expr, Type)> {
        let (mut left, ty) = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(TokenKind::Star) => BinOp::Mul,
                Some(TokenKind::Slash) => BinOp::Div,
                _ => return Ok((left, ty)),
            };
            self.pos += 1;
            let (right, rty) = self.unary()?;
            if ty != Type::Num || rty != Type::Num {
                return self.error();
            }
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> PResult<(Expr, Type)> {
        let neg = match self.peek() {
            Some(TokenKind::Minus) => true,
            Some(TokenKind::Plus) => false,
            _ => return self.power(),
        };
        self.pos += 1;
        let (operand, ty) = self.unary()?;
        if ty != Type::Num {
            return self.error();
        }
        let expr = if neg {
            Expr::Unary(UnOp::Neg, Box::new(operand))
        } else {
            operand
        };
        Ok((expr, Type::Num))
    }

    /// `^` binds tighter than unary minus and is right-associative;
    /// its right operand may carry its own sign (`2^-1`).
    fn power(&mut self) -> PResult<(Expr, Type)> {
        let (base, ty) = self.primary()?;
        if !self.eat(&TokenKind::Caret) {
            return Ok((base, ty));
        }
        let (exponent, ety) = self.unary()?;
        if ty != Type::Num || ety != Type::Num {
            return self.error();
        }
        Ok((
            Expr::Binary(BinOp::Pow, Box::new(base), Box::new(exponent)),
            Type::Num,
        ))
    }

    fn primary(&mut self) -> PResult<(Expr, Type)> {
        let (expr, ty) = match self.advance() {
            Some(TokenKind::Number(n)) => (Expr::Num(*n), Type::Num),
            Some(TokenKind::Str(s)) => (Expr::Str(s.clone()), Type::Str),
            Some(TokenKind::LParen) => {
                let inner = self.expr()?;
                self.expect(&TokenKind::RParen)?;
                inner
            }
            Some(TokenKind::Ident(name)) => {
                let name = name.clone();
                match type_of_name(&name) {
                    Type::Num => {
                        let subs = if self.eat(&TokenKind::LParen) {
                            Some(self.num_args()?)
                        } else {
                            None
                        };
                        (Expr::NumVar(name, subs), Type::Num)
                    }
                    Type::Str => {
                        let sub = if self.eat(&TokenKind::LParen) {
                            Some(self.subscript()?)
                        } else {
                            None
                        };
                        (Expr::StrVar(name, sub), Type::Str)
                    }
                }
            }
            Some(TokenKind::Keyword(kw)) => self.function(*kw)?,
            _ => return self.error(),
        };
        if ty == Type::Str {
            return self.string_postfix(expr);
        }
        Ok((expr, ty))
    }

    /// Slices applied to a string value: `"hello"(2 TO 3)`, `a$(2)(1)`.
    fn string_postfix(&mut self, mut expr: Expr) -> PResult<(Expr, Type)> {
        while self.peek() == Some(&TokenKind::LParen) {
            self.pos += 1;
            let sub = self.subscript()?;
            if matches!(&sub, Subscript::Index(args) if args.len() != 1) {
                return self.error();
            }
            expr = Expr::Slice(Box::new(expr), sub);
        }
        Ok((expr, Type::Str))
    }

    fn function(&mut self, kw: Keyword) -> PResult<(Expr, Type)> {
        let (arg_ty, result_ty) = match kw {
            Keyword::Rnd => return Ok((Expr::Func(kw, None), Type::Num)),
            Keyword::InkeyS => return Ok((Expr::Func(kw, None), Type::Str)),
            Keyword::Abs
            | Keyword::Acs
            | Keyword::Asn
            | Keyword::Atn
            | Keyword::Cos
            | Keyword::Exp
            | Keyword::Int
            | Keyword::Ln
            | Keyword::Sgn
            | Keyword::Sin
            | Keyword::Sqr
            | Keyword::Tan => (Type::Num, Type::Num),
            Keyword::ChrS | Keyword::StrS => (Type::Num, Type::Str),
            Keyword::Code | Keyword::Len | Keyword::Val => (Type::Str, Type::Num),
            _ => return self.error(),
        };
        let (arg, ty) = self.function_arg()?;
        if ty != arg_ty {
            return self.error();
        }
        Ok((Expr::Func(kw, Some(Box::new(arg))), result_ty))
    }

    /// Functions bind tighter than any operator (`SIN x^2` is `(SIN x)^2`),
    /// but their argument may carry a sign (`ABS -3`).
    fn function_arg(&mut self) -> PResult<(Expr, Type)> {
        let neg = match self.peek() {
            Some(TokenKind::Minus) => true,
            Some(TokenKind::Plus) => false,
            _ => return self.primary(),
        };
        self.pos += 1;
        let (arg, ty) = self.function_arg()?;
        if ty != Type::Num {
            return self.error();
        }
        let arg = if neg {
            Expr::Unary(UnOp::Neg, Box::new(arg))
        } else {
            arg
        };
        Ok((arg, Type::Num))
    }
}

pub fn type_of_name(name: &str) -> Type {
    if name.ends_with('$') {
        Type::Str
    } else {
        Type::Num
    }
}

/// Splits raw `DATA` text on commas outside quotes.
fn split_data(raw: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut in_quotes = false;
    let mut start = 0;
    for (i, c) in raw.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                parts.push(&raw[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&raw[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Vec<Stmt> {
        parse_line(src).unwrap_or_else(|e| panic!("{src:?} failed at statement {}", e.statement))
    }

    fn err(src: &str) -> u16 {
        parse_line(src)
            .map(|s| panic!("{src:?} parsed as {s:?}"))
            .unwrap_err()
            .statement
    }

    #[test]
    fn multi_statement_lines() {
        assert_eq!(ok("LET x=1 : PRINT x").len(), 2);
    }

    #[test]
    fn if_then_flattens_following_statements() {
        let stmts = ok("IF a=1 THEN PRINT 1: PRINT 2");
        assert!(matches!(stmts[0], Stmt::If(_)));
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn exponent_binds_tighter_than_unary_minus() {
        let stmts = ok("LET x=-2^2");
        let Stmt::Let(_, expr) = &stmts[0] else {
            panic!()
        };
        assert!(matches!(expr, Expr::Unary(UnOp::Neg, _)));
    }

    #[test]
    fn string_slices() {
        ok("PRINT a$(2 TO 3); a$(TO 2); a$(3 TO); a$(1); \"hello\"(2 TO)");
    }

    #[test]
    fn type_errors_are_nonsense() {
        assert_eq!(err(r#"LET a="x""#), 1);
        assert_eq!(err("PRINT 1: LET a$=1"), 2);
        assert_eq!(err(r#"PRINT "a"-"b""#), 1);
        assert_eq!(err(r#"IF "a" THEN PRINT 1"#), 1);
    }

    #[test]
    fn let_is_mandatory() {
        assert_eq!(err("x=1"), 1);
    }

    #[test]
    fn missing_then_is_rejected() {
        assert_eq!(err("IF x PRINT 1"), 1);
        assert_eq!(err("IF x THEN"), 1);
    }

    #[test]
    fn unclosed_parenthesis_is_rejected() {
        assert_eq!(err("PRINT 1: PRINT (1+2"), 2);
    }

    #[test]
    fn print_needs_separators_between_items() {
        assert_eq!(err("PRINT 1 2"), 1);
        ok("PRINT 1;2,3'4;");
    }

    #[test]
    fn data_items() {
        let stmts = ok(r#"DATA 1, apple pie, "x,""y""""#);
        assert_eq!(
            stmts[0],
            Stmt::Data(vec![
                DataItem::Raw("1".into()),
                DataItem::Raw("apple pie".into()),
                DataItem::Quoted("x,\"y\"".into()),
            ])
        );
    }

    #[test]
    fn numeric_expression_helper() {
        assert!(parse_numeric_expr("1+2*3").is_some());
        assert!(parse_numeric_expr("\"a\"").is_none());
        assert!(parse_numeric_expr("1 2").is_none());
    }
}
