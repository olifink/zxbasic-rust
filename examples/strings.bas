10 REM String Manipulation Demo
20 LET a$ = "SINCLAIR ZX SPECTRUM"
30 PRINT "Full string: "; a$
40 PRINT "Slice 1 to 8: "; a$(1 TO 8)
50 PRINT "Slice 10 to 11: "; a$(10 TO 11)
60 PRINT "Slice 13 to end: "; a$(13 TO)
70 PRINT "Single char (5): "; a$(5)
80 PRINT "Length: "; LEN(a$)
90 LET s$ = "EQUAL" AND (LEN(a$) = 20)
100 PRINT "Sinclair AND result: "; s$
110 STOP
