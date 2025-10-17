IDENTIFICATION DIVISION.
       PROGRAM-ID. SIMPLEADD.
       AUTHOR. STEFVUCK.
       
       ENVIRONMENT DIVISION.
       
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-RESULT       PIC S9(9)V99.
       
       LINKAGE SECTION.
       01  INPUT-NUM1      PIC S9(9)V99.
       01  INPUT-NUM2      PIC S9(9)V99.
       01  OUTPUT-RESULT   PIC S9(9)V99.
       01  RETURN-CODE-OUT PIC S9(4).
       
       PROCEDURE DIVISION USING INPUT-NUM1
                                INPUT-NUM2
                                OUTPUT-RESULT
                                RETURN-CODE-OUT.
       
       MAIN-LOGIC.
           ADD INPUT-NUM1 TO INPUT-NUM2 GIVING WS-RESULT.
           MOVE WS-RESULT TO OUTPUT-RESULT.
           MOVE 0 TO RETURN-CODE-OUT.
           GOBACK.
