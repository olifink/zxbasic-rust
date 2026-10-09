//! Tokenizer and keyword recognizer (BASIC-SPECS.md §1, SPECS-v2.md §1.1).

/// Reserved words: statements, operators and built-in functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keyword {
    // Statements and commands
    Auto,
    Clear,
    Cls,
    Data,
    Dim,
    Edit,
    Exit,
    For,
    Gosub,
    Goto,
    If,
    Input,
    Let,
    List,
    Load,
    New,
    Next,
    Print,
    Read,
    Rem,
    Renum,
    Restore,
    Return,
    Run,
    Save,
    Step,
    Stop,
    Then,
    To,
    // Logical operators
    And,
    Not,
    Or,
    // Functions
    Abs,
    Acs,
    Asn,
    Atn,
    ChrS,
    Code,
    Cos,
    Exp,
    InkeyS,
    Int,
    Len,
    Ln,
    Rnd,
    Sgn,
    Sin,
    Sqr,
    StrS,
    Tan,
    Val,
}

const KEYWORDS: &[(&str, Keyword)] = &[
    ("ABS", Keyword::Abs),
    ("ACS", Keyword::Acs),
    ("AND", Keyword::And),
    ("ASN", Keyword::Asn),
    ("ATN", Keyword::Atn),
    ("AUTO", Keyword::Auto),
    ("CHR$", Keyword::ChrS),
    ("CLEAR", Keyword::Clear),
    ("CLS", Keyword::Cls),
    ("CODE", Keyword::Code),
    ("COS", Keyword::Cos),
    ("DATA", Keyword::Data),
    ("DIM", Keyword::Dim),
    ("EDIT", Keyword::Edit),
    ("EXIT", Keyword::Exit),
    ("EXP", Keyword::Exp),
    ("FOR", Keyword::For),
    ("GOSUB", Keyword::Gosub),
    ("GOTO", Keyword::Goto),
    ("IF", Keyword::If),
    ("INKEY$", Keyword::InkeyS),
    ("INPUT", Keyword::Input),
    ("INT", Keyword::Int),
    ("LEN", Keyword::Len),
    ("LET", Keyword::Let),
    ("LIST", Keyword::List),
    ("LN", Keyword::Ln),
    ("LOAD", Keyword::Load),
    ("NEW", Keyword::New),
    ("NEXT", Keyword::Next),
    ("NOT", Keyword::Not),
    ("OR", Keyword::Or),
    ("PRINT", Keyword::Print),
    ("READ", Keyword::Read),
    ("REM", Keyword::Rem),
    ("RENUM", Keyword::Renum),
    ("RESTORE", Keyword::Restore),
    ("RETURN", Keyword::Return),
    ("RND", Keyword::Rnd),
    ("RUN", Keyword::Run),
    ("SAVE", Keyword::Save),
    ("SGN", Keyword::Sgn),
    ("SIN", Keyword::Sin),
    ("SQR", Keyword::Sqr),
    ("STEP", Keyword::Step),
    ("STOP", Keyword::Stop),
    ("STR$", Keyword::StrS),
    ("TAN", Keyword::Tan),
    ("THEN", Keyword::Then),
    ("TO", Keyword::To),
    ("VAL", Keyword::Val),
];

impl Keyword {
    /// Case-insensitive keyword lookup.
    pub fn lookup(word: &str) -> Option<Keyword> {
        KEYWORDS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(word))
            .map(|&(_, kw)| kw)
    }

    /// Canonical uppercase spelling.
    pub fn name(self) -> &'static str {
        KEYWORDS
            .iter()
            .find(|&&(_, kw)| kw == self)
            .map(|&(name, _)| name)
            .unwrap_or("?")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Number(f64),
    Str(String),
    /// Variable name; string variables keep their trailing `$`.
    Ident(String),
    Keyword(Keyword),
    /// Raw text of a `DATA` statement, up to the next unquoted colon.
    Data(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    LParen,
    RParen,
    Comma,
    Semicolon,
    Colon,
    Apostrophe,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// Byte range of the token in the source text.
    pub start: usize,
    pub end: usize,
}

/// A lexical error, located by its 1-based statement index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexError {
    pub statement: u16,
}

/// Splits one line of source (without its line number) into tokens.
///
/// Everything after `REM` is ignored and produces no tokens.
pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut statement: u16 = 1;
    let mut i = 0;
    let err = |statement| Err(LexError { statement });

    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        let kind = if c.is_ascii_digit() || (c == b'.' && next_is_digit(bytes, i + 1)) {
            i = scan_number(bytes, i);
            match src[start..i].parse::<f64>() {
                Ok(n) => TokenKind::Number(n),
                Err(_) => return err(statement),
            }
        } else if c.is_ascii_alphabetic() {
            while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'$' {
                i += 1;
            }
            let word = &src[start..i];
            match Keyword::lookup(word) {
                Some(Keyword::Rem) => {
                    tokens.push(Token {
                        kind: TokenKind::Keyword(Keyword::Rem),
                        start,
                        end: i,
                    });
                    return Ok(tokens);
                }
                Some(Keyword::Data) => {
                    tokens.push(Token {
                        kind: TokenKind::Keyword(Keyword::Data),
                        start,
                        end: i,
                    });
                    let data_start = i;
                    i = match scan_data(bytes, i) {
                        Some(end) => end,
                        None => return err(statement),
                    };
                    tokens.push(Token {
                        kind: TokenKind::Data(src[data_start..i].to_string()),
                        start: data_start,
                        end: i,
                    });
                    continue;
                }
                Some(kw) => TokenKind::Keyword(kw),
                None => TokenKind::Ident(word.to_string()),
            }
        } else if c == b'"' {
            let (value, end) = match scan_string(src, i) {
                Some(found) => found,
                None => return err(statement),
            };
            i = end;
            TokenKind::Str(value)
        } else {
            i += 1;
            match c {
                b'+' => TokenKind::Plus,
                b'-' => TokenKind::Minus,
                b'*' => TokenKind::Star,
                b'/' => TokenKind::Slash,
                b'^' => TokenKind::Caret,
                b'=' => TokenKind::Eq,
                b'(' => TokenKind::LParen,
                b')' => TokenKind::RParen,
                b',' => TokenKind::Comma,
                b';' => TokenKind::Semicolon,
                b'\'' => TokenKind::Apostrophe,
                b':' => {
                    statement = statement.saturating_add(1);
                    TokenKind::Colon
                }
                b'<' => match bytes.get(i) {
                    Some(b'>') => {
                        i += 1;
                        TokenKind::Ne
                    }
                    Some(b'=') => {
                        i += 1;
                        TokenKind::Le
                    }
                    _ => TokenKind::Lt,
                },
                b'>' => match bytes.get(i) {
                    Some(b'=') => {
                        i += 1;
                        TokenKind::Ge
                    }
                    _ => TokenKind::Gt,
                },
                _ => return err(statement),
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: i,
        });
    }
    Ok(tokens)
}

/// Rewrites keywords outside string literals and `REM` comments in canonical
/// uppercase, leaving everything else untouched (SPECS-v2.md §1.1).
pub fn canonicalize(src: &str, tokens: &[Token]) -> String {
    let mut out = String::with_capacity(src.len());
    let mut copied = 0;
    for token in tokens {
        if let TokenKind::Keyword(kw) = token.kind {
            out.push_str(&src[copied..token.start]);
            out.push_str(kw.name());
            copied = token.end;
        }
    }
    out.push_str(&src[copied..]);
    out
}

fn next_is_digit(bytes: &[u8], i: usize) -> bool {
    bytes.get(i).is_some_and(u8::is_ascii_digit)
}

fn scan_number(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    // Exponent: only when followed by digits, so `1E` stays a number and a name.
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        if next_is_digit(bytes, j) {
            i = j;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    i
}

/// Scans a string literal starting at the opening quote. Returns the
/// unescaped value and the index just past the closing quote.
fn scan_string(src: &str, start: usize) -> Option<(String, usize)> {
    let bytes = src.as_bytes();
    let mut value = String::new();
    let mut i = start + 1;
    let mut chunk = i;
    loop {
        match bytes.get(i)? {
            b'"' if bytes.get(i + 1) == Some(&b'"') => {
                value.push_str(&src[chunk..=i]);
                i += 2;
                chunk = i;
            }
            b'"' => {
                value.push_str(&src[chunk..i]);
                return Some((value, i + 1));
            }
            _ => i += 1,
        }
    }
}

/// Finds the end of a `DATA` item list: the next colon outside quotes.
fn scan_data(bytes: &[u8], mut i: usize) -> Option<usize> {
    let mut in_quotes = false;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b':' if !in_quotes => break,
            _ => {}
        }
        i += 1;
    }
    (!in_quotes).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn keywords_are_case_insensitive() {
        assert_eq!(
            kinds("print Print PRINT inkey$"),
            [
                TokenKind::Keyword(Keyword::Print),
                TokenKind::Keyword(Keyword::Print),
                TokenKind::Keyword(Keyword::Print),
                TokenKind::Keyword(Keyword::InkeyS),
            ]
        );
    }

    #[test]
    fn identifiers_keep_case_and_dollar() {
        assert_eq!(
            kinds("total NAME$ a1"),
            [
                TokenKind::Ident("total".into()),
                TokenKind::Ident("NAME$".into()),
                TokenKind::Ident("a1".into()),
            ]
        );
    }

    #[test]
    fn numbers_and_operators() {
        assert_eq!(
            kinds("1.5E3<>.5<=2"),
            [
                TokenKind::Number(1500.0),
                TokenKind::Ne,
                TokenKind::Number(0.5),
                TokenKind::Le,
                TokenKind::Number(2.0),
            ]
        );
    }

    #[test]
    fn doubled_quotes_escape_a_quote() {
        assert_eq!(
            kinds(r#""He said, ""Hi!""""#),
            [TokenKind::Str(r#"He said, "Hi!""#.into())]
        );
    }

    #[test]
    fn unterminated_string_reports_statement() {
        assert_eq!(
            tokenize(r#"LET a=1: PRINT "oops"#),
            Err(LexError { statement: 2 })
        );
    }

    #[test]
    fn rem_swallows_rest_of_line() {
        assert_eq!(
            kinds("PRINT 1: REM a: b \"c"),
            [
                TokenKind::Keyword(Keyword::Print),
                TokenKind::Number(1.0),
                TokenKind::Colon,
                TokenKind::Keyword(Keyword::Rem),
            ]
        );
    }

    #[test]
    fn data_is_kept_raw_up_to_colon() {
        assert_eq!(
            kinds(r#"DATA 1, apple, "a:b": PRINT"#),
            [
                TokenKind::Keyword(Keyword::Data),
                TokenKind::Data(r#" 1, apple, "a:b""#.into()),
                TokenKind::Colon,
                TokenKind::Keyword(Keyword::Print),
            ]
        );
    }

    #[test]
    fn canonicalize_uppercases_keywords_only() {
        let src = r#"for i=1 to 10: print "count: "; i: next i: rem keep this"#;
        let tokens = tokenize(src).unwrap();
        assert_eq!(
            canonicalize(src, &tokens),
            r#"FOR i=1 TO 10: PRINT "count: "; i: NEXT i: REM keep this"#
        );
    }
}
