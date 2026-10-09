10 REM Fibonacci Series Demo
20 PRINT "Fibonacci Series:"
30 LET a = 0
40 LET b = 1
50 FOR i = 1 TO 10
60 PRINT a; " ";
70 LET nxt = a + b
80 LET a = b
90 LET b = nxt
100 NEXT i
110 PRINT ""
120 STOP
