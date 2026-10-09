//! End-to-end tests: scripts are fed through the REPL as piped input.

use std::cell::RefCell;
use std::io::{self, Cursor, Write};
use std::rc::Rc;

use zxbasic::input::PlainSource;
use zxbasic::terminal::Console;
use zxbasic::{Interpreter, Output, Repl};

#[derive(Clone, Default)]
struct Shared(Rc<RefCell<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Shared {
    fn text(&self) -> String {
        String::from_utf8(self.0.borrow().clone()).unwrap()
    }
}

struct Run {
    out: String,
    err: String,
    status: i32,
}

fn run(script: &str) -> Run {
    let out = Shared::default();
    let err = Shared::default();
    let interpreter = Interpreter::new(
        Output::new(Box::new(out.clone()), false),
        Box::new(err.clone()),
        Console::disabled(),
        false,
    );
    let source = PlainSource::new(Cursor::new(script.as_bytes().to_vec()));
    let status = Repl::new(interpreter, Box::new(source)).run();
    Run {
        out: out.text(),
        err: err.text(),
        status,
    }
}

/// Runs a script and expects it to succeed with exactly `expected` on stdout.
fn output(script: &str) -> String {
    let r = run(script);
    assert_eq!(r.err, "", "unexpected report for script:\n{script}");
    r.out
}

/// Runs a script and returns its reports (stderr).
fn reports(script: &str) -> String {
    run(script).err
}

// ----- program store & ingestion -----

#[test]
fn hello_world() {
    assert_eq!(
        output("10 PRINT \"Hello, world!\"\nRUN\n"),
        "Hello, world!\n"
    );
}

#[test]
fn lines_are_listed_in_order_with_canonical_keywords() {
    let script = "20 print \"Done\": rem keep case\n10 for i=1 to 3: print i;: next i\nLIST\n";
    assert_eq!(
        output(script),
        "10 FOR i=1 TO 3: PRINT i;: NEXT i\n20 PRINT \"Done\": REM keep case\n"
    );
}

#[test]
fn list_from_line() {
    assert_eq!(
        output("10 REM a\n20 REM b\n30 REM c\nLIST 15\n"),
        "20 REM b\n30 REM c\n"
    );
}

#[test]
fn bare_line_number_deletes_line() {
    assert_eq!(output("10 REM a\n20 REM b\n10\nLIST\n"), "20 REM b\n");
}

#[test]
fn rejected_line_leaves_store_untouched() {
    let r = run("10 PRINT 1\n10 PRINT (1+2\n20 LET a=1: PRINT \"x\nLIST\n");
    assert_eq!(r.out, "10 PRINT 1\n");
    assert_eq!(
        r.err,
        "C Nonsense in BASIC, 10:1\nC Nonsense in BASIC, 20:2\n"
    );
}

#[test]
fn line_numbers_out_of_range() {
    assert_eq!(
        reports("0 PRINT 1\n10000 PRINT 1\n"),
        "B Integer out of range\nB Integer out of range\n"
    );
}

#[test]
fn immediate_errors_have_no_position() {
    assert_eq!(
        reports("x=1\nPRINT nope\n"),
        "C Nonsense in BASIC\n2 Variable not found\n"
    );
}

#[test]
fn new_clears_everything() {
    let r = run("10 PRINT 1\nLET a=1\nNEW\nLIST\nPRINT a\n");
    assert_eq!(r.out, "");
    assert_eq!(r.err, "2 Variable not found\n");
}

// ----- PRINT & expressions -----

#[test]
fn print_separators() {
    assert_eq!(output("PRINT 1;2\n"), "12\n");
    assert_eq!(
        output("PRINT \"a\",\"b\"\n"),
        format!("a{}b\n", " ".repeat(15))
    );
    assert_eq!(output("PRINT ,\"x\"\n"), format!("{}x\n", " ".repeat(16)));
    assert_eq!(output("PRINT \"a\"'\"b\"\n"), "a\nb\n");
    assert_eq!(output("PRINT \"a\";\nPRINT \"b\"\n"), "ab\n");
    assert_eq!(output("PRINT\n"), "\n");
}

#[test]
fn operator_precedence() {
    assert_eq!(
        output("PRINT 2+3*4'-2^2'2^3^2'10/4'(1+2)*3'2^-1'7-2-1\n"),
        "14\n-4\n512\n2.5\n9\n.5\n4\n"
    );
}

#[test]
fn relational_and_logical_operators() {
    assert_eq!(
        output("PRINT 1<2;2<1;\"a\"<\"b\";\"x\"=\"x\";3<>3;2>=2\n"),
        "101101\n"
    );
    assert_eq!(output("PRINT NOT 0;NOT 5;NOT 1=2\n"), "101\n");
    assert_eq!(output("PRINT 5 AND 1;\",\";5 AND 0\n"), "5,0\n");
    assert_eq!(output("PRINT 0 OR 7;\",\";3 OR 7\n"), "7,3\n");
    assert_eq!(
        output("PRINT \"yes\" AND 1;\"|\";\"no\" AND 0;\"|\"\n"),
        "yes||\n"
    );
}

#[test]
fn number_formatting() {
    assert_eq!(
        output("PRINT 1/3'1/4'-0.5'1E20'100000*100000'0.1+0.2\n"),
        ".33333333\n.25\n-.5\n1E+20\n10000000000\n.3\n"
    );
}

#[test]
fn math_functions() {
    assert_eq!(
        output("PRINT INT 2.7;\" \";INT -2.5;\" \";ABS -3;\" \";SGN -9;\" \";SGN 0;\" \";SQR 16\n"),
        "2 -3 3 -1 0 4\n"
    );
    assert_eq!(
        output("PRINT EXP 0;LN 1;ATN 0;COS 0;ACS 1;ASN 0;TAN 0\n"),
        "1001000\n"
    );
    assert_eq!(output("PRINT INT (SQR 2*1000)\n"), "1414\n");
}

#[test]
fn rnd_is_in_unit_interval() {
    let script = "10 LET ok=1: FOR i=1 TO 500: LET r=RND\n20 IF r<0 OR r>=1 THEN LET ok=0\n30 NEXT i: PRINT ok\nRUN\n";
    assert_eq!(output(script), "1\n");
}

#[test]
fn string_functions() {
    assert_eq!(
        output("PRINT LEN \"hello\";CODE \"A\";CODE \"\";CHR$ 66;STR$ 1.5;VAL \"2*3+1\"\n"),
        "5650B1.57\n"
    );
    assert_eq!(output("PRINT STR$ 12 + \"!\"\n"), "12!\n");
}

#[test]
fn string_concatenation_and_quotes() {
    assert_eq!(
        output("LET a$=\"He said \"\"hi\"\"\": PRINT a$ + \"!\"\n"),
        "He said \"hi\"!\n"
    );
}

#[test]
fn string_slicing() {
    let script = "LET a$=\"Hello World\"\nPRINT a$(1 TO 5);\"|\";a$(7 TO);\"|\";a$(TO 4);\"|\";a$(5);\"|\";a$(5 TO 2);\"|\";\"abc\"(2)\n";
    assert_eq!(output(script), "Hello|World|Hell|o||b\n");
    assert_eq!(
        reports("LET a$=\"abc\": PRINT a$(2 TO 9)\n"),
        "3 Subscript out of range\n"
    );
}

#[test]
fn substring_assignment_pads_and_truncates() {
    assert_eq!(
        output(
            "LET a$=\"Hello\": LET a$(1 TO 3)=\"J\": PRINT a$;\"|\": LET a$(2)=\"XYZ\": PRINT a$\n"
        ),
        "J  lo|\nJX lo\n"
    );
}

#[test]
fn variables_are_case_sensitive_and_multi_letter() {
    assert_eq!(
        output(
            "LET a=1: LET A=2: LET total=3: LET name$=\"Bob\": LET NAME$=\"Al\"\nPRINT a;A;total;name$;NAME$\n"
        ),
        "123BobAl\n"
    );
}

#[test]
fn arithmetic_errors() {
    assert_eq!(reports("PRINT 1/0\n"), "6 Number too big\n");
    assert_eq!(reports("PRINT SQR -1\n"), "A Invalid argument\n");
    assert_eq!(reports("PRINT LN 0\n"), "A Invalid argument\n");
    assert_eq!(reports("PRINT 10^400\n"), "6 Number too big\n");
    assert_eq!(reports("PRINT CHR$ 300\n"), "B Integer out of range\n");
    assert_eq!(reports("PRINT VAL \"x+\"\n"), "C Nonsense in BASIC\n");
}

// ----- control flow -----

#[test]
fn goto_and_if() {
    let script = "10 LET n=1\n20 PRINT n;\n30 LET n=n+1\n40 IF n<=5 THEN GOTO 20\n50 PRINT\nRUN\n";
    assert_eq!(output(script), "12345\n");
}

#[test]
fn false_if_skips_rest_of_line() {
    assert_eq!(
        output("10 IF 0 THEN PRINT \"a\": PRINT \"b\"\n20 PRINT \"c\"\nRUN\n"),
        "c\n"
    );
}

#[test]
fn error_after_then_counts_as_later_statement() {
    assert_eq!(
        reports("10 IF 1 THEN PRINT zz\nRUN\n"),
        "2 Variable not found, 10:2\n"
    );
}

#[test]
fn goto_missing_line() {
    assert_eq!(reports("10 GOTO 99\nRUN\n"), "N Statement lost, 10:1\n");
}

#[test]
fn stop_halts_with_report() {
    let r = run("10 PRINT \"a\"\n20 STOP\n30 PRINT \"b\"\nRUN\n");
    assert_eq!(r.out, "a\n");
    assert_eq!(r.err, "9 STOP statement, 20:1\n");
}

#[test]
fn for_next_loops() {
    assert_eq!(output("FOR i=1 TO 5: PRINT i;: NEXT i: PRINT\n"), "12345\n");
    assert_eq!(
        output("FOR i=10 TO 1 STEP -3: PRINT i;\" \";: NEXT i: PRINT\n"),
        "10 7 4 1 \n"
    );
    assert_eq!(
        output("FOR i=0 TO 1 STEP .25: PRINT i;\" \";: NEXT i: PRINT\n"),
        "0 .25 .5 .75 1 \n"
    );
}

#[test]
fn nested_loops_across_lines() {
    let script =
        "10 FOR i=1 TO 3\n20 FOR j=1 TO i\n30 PRINT \"*\";\n40 NEXT j\n50 PRINT\n60 NEXT i\nRUN\n";
    assert_eq!(output(script), "*\n**\n***\n");
}

#[test]
fn zero_iteration_loop_is_skipped() {
    let script = "10 FOR i=5 TO 1\n20 PRINT \"never\"\n30 NEXT i\n40 PRINT i\nRUN\n";
    assert_eq!(output(script), "5\n");
}

#[test]
fn next_without_for() {
    assert_eq!(reports("10 NEXT i\nRUN\n"), "1 NEXT without FOR, 10:1\n");
}

#[test]
fn gosub_and_return() {
    let script = "10 GOSUB 100: PRINT \"back\"\n20 STOP\n100 PRINT \"sub\"\n110 RETURN\nRUN\n";
    let r = run(script);
    assert_eq!(r.out, "sub\nback\n");
    assert_eq!(r.err, "9 STOP statement, 20:1\n");
    assert_eq!(
        reports("10 RETURN\nRUN\n"),
        "7 Return without GOSUB, 10:1\n"
    );
}

#[test]
fn immediate_goto_enters_program_without_clearing() {
    assert_eq!(output("10 PRINT a\nLET a=7\nGOTO 10\n"), "7\n");
    assert_eq!(
        reports("10 PRINT a\nLET a=7\nRUN\n"),
        "2 Variable not found, 10:1\n"
    );
}

#[test]
fn run_from_line() {
    assert_eq!(output("10 PRINT 1\n20 PRINT 2\nRUN 20\n"), "2\n");
}

#[test]
fn clear_removes_variables() {
    assert_eq!(
        reports("LET a=1\nCLEAR\nPRINT a\n"),
        "2 Variable not found\n"
    );
}

// ----- DATA / READ / RESTORE -----

#[test]
fn data_read_restore() {
    let script = "10 DATA 1, 2.5, \"three\", four\n20 READ a, b, c$, d$\n30 PRINT a;b;c$;d$\n40 RESTORE\n50 READ x: PRINT x\n60 RESTORE 100\n70 READ y$: PRINT y$\n100 DATA \"last\"\nRUN\n";
    assert_eq!(output(script), "12.5threefour\n1\nlast\n");
}

#[test]
fn end_of_data() {
    assert_eq!(
        reports("10 DATA 1\n20 READ a, b\nRUN\n"),
        "8 End of DATA, 20:1\n"
    );
}

#[test]
fn reading_text_into_number_is_nonsense() {
    assert_eq!(
        reports("10 DATA \"x\"\n20 READ a\nRUN\n"),
        "C Nonsense in BASIC, 20:1\n"
    );
}

// ----- arrays -----

#[test]
fn numeric_and_string_arrays() {
    assert_eq!(
        output("DIM m(3,3): LET m(2,3)=5: PRINT m(2,3);m(1,1)\n"),
        "50\n"
    );
    assert_eq!(
        reports("DIM a(2): PRINT a(3)\n"),
        "3 Subscript out of range\n"
    );
    assert_eq!(
        reports("DIM a(2): PRINT a(1,1)\n"),
        "3 Subscript out of range\n"
    );
    assert_eq!(reports("DIM a(2): DIM a(3)\n"), "C Nonsense in BASIC\n");
    assert_eq!(reports("PRINT b(1)\n"), "2 Variable not found\n");
}

#[test]
fn string_arrays_are_character_matrices() {
    // The last dimension is the fixed string length; rows start as spaces.
    let script = concat!(
        "DIM c$(2,5)\n",
        "PRINT \"[\";c$(1);\"]\"\n",
        "LET c$(1)=\"ABCDEFG\": PRINT \"[\";c$(1);\"]\"\n",
        "LET c$(2)=\"XY\": PRINT \"[\";c$(2);\"]\"\n",
        "PRINT c$(1,2);\"|\";c$(1,2 TO 4);\"|\";c$(1,TO 2);\"|\";c$(1)(5)\n",
        "LET c$(2,4)=\"Z\": LET c$(1,2 TO 3)=\"*\": PRINT c$(2);\"|\";c$(1)\n",
        "PRINT LEN c$(2)\n",
    );
    assert_eq!(
        output(script),
        "[     ]\n[ABCDE]\n[XY   ]\nB|BCD|AB|E\nXY Z |A* DE\n5\n"
    );
    assert_eq!(
        reports("DIM c$(2,5): PRINT c$(3)\n"),
        "3 Subscript out of range\n"
    );
    assert_eq!(
        reports("DIM c$(2,5): PRINT c$(1,6)\n"),
        "3 Subscript out of range\n"
    );
    assert_eq!(
        reports("DIM c$(2,5): PRINT c$(1 TO 2)\n"),
        "3 Subscript out of range\n"
    );
}

#[test]
fn one_dimensional_string_array_is_a_fixed_length_string() {
    assert_eq!(
        output("DIM s$(5): LET s$=\"Hi\": PRINT \"[\";s$;\"]\";s$(2);LEN s$\n"),
        "[Hi   ]i5\n"
    );
}

#[test]
fn arrays_example() {
    let r = run(&format!("{}RUN\n", include_str!("../examples/arrays.bas")));
    assert_eq!(
        r.out,
        concat!(
            "Matrix element a(2, 3) = 23\n",
            "Name 1: [ALICE     ]\n",
            "Name 2: [BOB       ]\n",
            "Name 3: [CHARLIE   ]\n",
        )
    );
    assert_eq!(r.err, "9 STOP statement, 160:1\n");
}

#[test]
fn strings_example() {
    let r = run(&format!("{}RUN\n", include_str!("../examples/strings.bas")));
    assert_eq!(
        r.out,
        concat!(
            "Full string: SINCLAIR ZX SPECTRUM\n",
            "Slice 1 to 8: SINCLAIR\n",
            "Slice 10 to 11: ZX\n",
            "Slice 13 to end: SPECTRUM\n",
            "Single char (5): L\n",
            "Length: 20\n",
            "Sinclair AND result: EQUAL\n",
        )
    );
    assert_eq!(r.err, "9 STOP statement, 110:1\n");
}

#[test]
fn fibonacci_example() {
    let r = run(&format!(
        "{}RUN\n",
        include_str!("../examples/fibonacci.bas")
    ));
    assert_eq!(r.out, "Fibonacci Series:\n0 1 1 2 3 5 8 13 21 34 \n");
    assert_eq!(r.err, "9 STOP statement, 120:1\n");
}

// ----- INPUT -----

#[test]
fn input_reads_following_lines() {
    let script = "10 INPUT \"Name? \"; n$: INPUT x\n20 PRINT n$; x*2\nRUN\nBob\n21\n";
    assert_eq!(output(script), "Name? Bob42\n");
}

#[test]
fn input_at_end_of_input() {
    assert_eq!(reports("10 INPUT a\nRUN\n"), "H STOP in INPUT, 10:1\n");
}

#[test]
fn input_rejects_non_numbers_when_piped() {
    assert_eq!(
        reports("10 INPUT a\nRUN\n1+\n"),
        "C Nonsense in BASIC, 10:1\n"
    );
}

#[test]
fn input_evaluates_replies_as_expressions() {
    // Like the Spectrum, a numeric INPUT reply may be an expression.
    assert_eq!(
        output("LET k=4\n10 INPUT a: PRINT a\nGOTO 10\nk*2\n"),
        "8\n"
    );
}

// ----- SAVE / LOAD -----

#[test]
fn save_and_load_round_trip() {
    let path = std::env::temp_dir().join(format!("zxbasic-test-{}.bas", std::process::id()));
    let path = path.to_str().unwrap();
    let script = format!(
        "20 PRINT \"world\"\n10 print \"hello\"\nSAVE \"{path}\"\nNEW\nLOAD \"{path}\"\nLIST\nRUN\n"
    );
    let out = output(&script);
    let saved = std::fs::read_to_string(path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(saved, "10 PRINT \"hello\"\n20 PRINT \"world\"\n");
    assert_eq!(
        out,
        "10 PRINT \"hello\"\n20 PRINT \"world\"\nhello\nworld\n"
    );
}

#[test]
fn load_missing_file_keeps_program() {
    let r = run("10 REM keep\nLOAD \"/nonexistent/zxbasic.bas\"\nLIST\n");
    assert_eq!(r.err, "F File not found\n");
    assert_eq!(r.out, "10 REM keep\n");
}

// ----- SPECS-v2 commands -----

#[test]
fn renum_rewrites_targets() {
    let script = "5 GOSUB 30\n7 GOTO 5\n30 RETURN\nRENUM\nLIST\nRENUM 100, 5\nLIST\n";
    assert_eq!(
        output(script),
        "10 GOSUB 30\n20 GOTO 10\n30 RETURN\n100 GOSUB 110\n105 GOTO 100\n110 RETURN\n"
    );
}

#[test]
fn renum_warns_about_missing_targets() {
    let r = run("10 GOTO 99\nRENUM 100\nLIST\n");
    assert_eq!(r.out, "100 GOTO 99\n");
    assert_eq!(r.err, "Warning: Line reference 99 not found at line 10\n");
}

#[test]
fn renum_out_of_range_leaves_program() {
    let r = run("10 REM a\n20 REM b\nRENUM 9995\nLIST\n");
    assert_eq!(r.err, "B Integer out of range\n");
    assert_eq!(r.out, "10 REM a\n20 REM b\n");
}

#[test]
fn auto_numbers_lines_until_blank() {
    assert_eq!(
        output("AUTO 100, 5\nprint 1\nPRINT 2\n\nLIST\n"),
        "100 PRINT 1\n105 PRINT 2\n"
    );
}

#[test]
fn auto_keeps_number_after_rejected_line() {
    let r = run("AUTO\nPRINT (\nPRINT 1\n\nLIST\n");
    assert_eq!(r.err, "C Nonsense in BASIC, 10:1\n");
    assert_eq!(r.out, "10 PRINT 1\n");
}

#[test]
fn edit_needs_a_terminal() {
    assert_eq!(reports("10 REM x\nEDIT 10\n"), "C Nonsense in BASIC\n");
}

#[test]
fn immediate_only_commands_fail_in_programs() {
    assert_eq!(reports("10 RENUM\nRUN\n"), "C Nonsense in BASIC, 10:1\n");
    assert_eq!(reports("10 AUTO\nRUN\n"), "C Nonsense in BASIC, 10:1\n");
}

#[test]
fn exit_status() {
    let r = run("PRINT 1\nEXIT 3\nPRINT 2\n");
    assert_eq!((r.out.as_str(), r.status), ("1\n", 3));
    let r = run("10 PRINT \"bye\": EXIT 7\n20 PRINT \"no\"\nRUN\n");
    assert_eq!((r.out.as_str(), r.status), ("bye\n", 7));
    assert_eq!(run("EXIT\n").status, 0);
    assert_eq!(run("PRINT 1\n").status, 0);
}

#[test]
fn errors_do_not_stop_a_script() {
    let r = run("PRINT nope\nPRINT 2\n");
    assert_eq!(r.out, "2\n");
    assert_eq!(r.status, 0);
}

#[test]
fn report_starts_on_a_fresh_line() {
    let r = run("10 PRINT \"x\";\n20 STOP\nRUN\n");
    assert_eq!(r.out, "x\n");
}
