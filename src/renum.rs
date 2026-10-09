//! Program renumbering (SPECS-v2.md §3).

use std::collections::BTreeMap;
use std::io::Write;

use crate::error::{BasicError, ErrorCode};
use crate::lexer::{self, Keyword, TokenKind};
use crate::program::{LineNo, MAX_LINE, Program};

/// Computes the renumbered program without modifying `program`.
///
/// Literal targets of `GOTO`, `GOSUB` and `RESTORE` are rewritten; references
/// to missing lines are left as-is with a warning. Fails with
/// `B Integer out of range` if any new line number would exceed 9999.
pub fn renumber(
    program: &Program,
    start: LineNo,
    step: LineNo,
    warnings: &mut dyn Write,
) -> Result<BTreeMap<LineNo, String>, BasicError> {
    // Pass 1: old -> new line mapping.
    let mut mapping = BTreeMap::new();
    for (i, (old, _)) in program.iter().enumerate() {
        let new = u32::from(start) + i as u32 * u32::from(step);
        if new > u32::from(MAX_LINE) {
            return Err(BasicError::immediate(ErrorCode::IntegerOutOfRange));
        }
        mapping.insert(old, new as LineNo);
    }

    // Pass 2: patch branch targets; pass 3: rebuild under the new numbers.
    let mut lines = BTreeMap::new();
    for (old, src) in program.iter() {
        let text = patch_targets(old, src, &mapping, warnings);
        lines.insert(mapping[&old], text);
    }
    Ok(lines)
}

fn patch_targets(
    old: LineNo,
    src: &str,
    mapping: &BTreeMap<LineNo, LineNo>,
    warnings: &mut dyn Write,
) -> String {
    let Ok(tokens) = lexer::tokenize(src) else {
        return src.to_string();
    };
    let mut out = String::with_capacity(src.len());
    let mut copied = 0;
    for (i, token) in tokens.iter().enumerate() {
        let TokenKind::Keyword(Keyword::Goto | Keyword::Gosub | Keyword::Restore) = token.kind
        else {
            continue;
        };
        let Some(target) = tokens.get(i + 1) else {
            continue;
        };
        let TokenKind::Number(value) = target.kind else {
            continue;
        };
        // Only a lone literal is a static target; `GOTO 100+x` is dynamic.
        if !matches!(
            tokens.get(i + 2).map(|t| &t.kind),
            None | Some(TokenKind::Colon)
        ) {
            continue;
        }
        let literal = &src[target.start..target.end];
        let new = (value.fract() == 0.0 && (0.0..=f64::from(MAX_LINE)).contains(&value))
            .then(|| mapping.get(&(value as LineNo)))
            .flatten();
        match new {
            Some(new) => {
                out.push_str(&src[copied..target.start]);
                out.push_str(&new.to_string());
                copied = target.end;
            }
            None => {
                let _ = writeln!(
                    warnings,
                    "Warning: Line reference {literal} not found at line {old}"
                );
            }
        }
    }
    out.push_str(&src[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(lines: &[(LineNo, &str)]) -> Program {
        let mut p = Program::new();
        for &(n, text) in lines {
            p.insert(n, text.to_string());
        }
        p
    }

    #[test]
    fn renumbers_and_patches_targets() {
        let p = program(&[
            (5, "GOSUB 30: RESTORE 30"),
            (7, "IF x THEN GOTO 5"),
            (30, "GOTO 5+x: RETURN"),
        ]);
        let mut warnings = Vec::new();
        let lines = renumber(&p, 100, 5, &mut warnings).unwrap();
        let listed: Vec<_> = lines.iter().map(|(n, s)| (*n, s.as_str())).collect();
        assert_eq!(
            listed,
            [
                (100, "GOSUB 110: RESTORE 110"),
                (105, "IF x THEN GOTO 100"),
                (110, "GOTO 5+x: RETURN"),
            ]
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn warns_about_missing_targets() {
        let p = program(&[(10, "GOTO 99")]);
        let mut warnings = Vec::new();
        let lines = renumber(&p, 10, 10, &mut warnings).unwrap();
        assert_eq!(lines[&10], "GOTO 99");
        assert_eq!(
            String::from_utf8(warnings).unwrap(),
            "Warning: Line reference 99 not found at line 10\n"
        );
    }

    #[test]
    fn aborts_when_exceeding_9999() {
        let p = program(&[(1, "REM a"), (2, "REM b")]);
        let err = renumber(&p, 9995, 10, &mut Vec::new()).unwrap_err();
        assert_eq!(err.code, ErrorCode::IntegerOutOfRange);
    }
}
