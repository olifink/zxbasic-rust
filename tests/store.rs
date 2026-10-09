//! Integration tests for the program line store.

use zxbasic::{ErrorCode, Program};

#[test]
fn lines_are_kept_in_ascending_order() {
    let mut program = Program::new();
    program.insert(30, "PRINT 3".into());
    program.insert(10, "PRINT 1".into());
    program.insert(20, "PRINT 2".into());

    let numbers: Vec<_> = program.iter().map(|(n, _)| n).collect();
    assert_eq!(numbers, [10, 20, 30]);
}

#[test]
fn insert_replaces_existing_line() {
    let mut program = Program::new();
    program.insert(10, "PRINT 1".into());
    program.insert(10, "PRINT 2".into());

    assert_eq!(program.len(), 1);
    assert_eq!(program.get(10), Some("PRINT 2"));
}

#[test]
fn delete_removes_line() {
    let mut program = Program::new();
    program.insert(10, "PRINT 1".into());

    assert_eq!(program.delete(10).as_deref(), Some("PRINT 1"));
    assert_eq!(program.delete(10), None);
    assert!(program.is_empty());
}

#[test]
fn iter_from_starts_at_or_after_line() {
    let mut program = Program::new();
    for n in [10, 20, 30] {
        program.insert(n, format!("REM {n}"));
    }

    let numbers: Vec<_> = program.iter_from(15).map(|(n, _)| n).collect();
    assert_eq!(numbers, [20, 30]);
}

#[test]
fn line_numbers_outside_range_are_rejected() {
    assert_eq!(Program::validate_line_no(1), Ok(1));
    assert_eq!(Program::validate_line_no(9999), Ok(9999));
    for bad in [0, 10_000, 70_000] {
        let err = Program::validate_line_no(bad).unwrap_err();
        assert_eq!(err.code, ErrorCode::IntegerOutOfRange);
    }
}
