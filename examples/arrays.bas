10 REM Array and String Matrix Demo
20 DIM a(3, 3)
30 FOR i = 1 TO 3
40 FOR j = 1 TO 3
50 LET a(i, j) = i * 10 + j
60 NEXT j
70 NEXT i
80 PRINT "Matrix element a(2, 3) = "; a(2, 3)
90 DIM names$(3, 10)
100 LET names$(1) = "ALICE"
110 LET names$(2) = "BOB"
120 LET names$(3) = "CHARLIE"
130 FOR k = 1 TO 3
140 PRINT "Name "; k; ": ["; names$(k); "]"
150 NEXT k
160 STOP
