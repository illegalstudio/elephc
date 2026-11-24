//! Purpose:
//! Holds the php-src `ext/tokenizer` corpus transcribed into Rust data.
//!
//! Called from:
//! - `tests/tokenizer_php_src.rs`, which compiles and runs every case.
//!
//! Key details:
//! - Transcribed from php-src `ext/tokenizer/tests` at PHP-8.5 commit
//!   `3323b8db05d51899069b7b715e10da6fc1da29d5` (2026-10-30).
//! - PHP License 3.01; the license text is in the sibling `LICENSE` file.
//! - `source` already has the trailing `?>` rewritten to `;`, as the compiler needs.
//! - `expected` is exact `--EXPECT--` text, or an `--EXPECTF--` pattern when
//!   `expectf` is true.

/// One php-src tokenizer test case.
pub(crate) struct Case {
    /// Original php-src test name, without the `.phpt` suffix.
    pub(crate) name: &'static str,
    /// PHP source compiled by the test.
    pub(crate) source: &'static str,
    /// Expected exact output or `EXPECTF` pattern.
    pub(crate) expected: &'static str,
    /// Whether `expected` is an `EXPECTF` pattern instead of exact text.
    pub(crate) expectf: bool,
}

/// Every transcribed php-src tokenizer case.
pub(crate) const CASES: &[Case] = &[
    Case {
        name: r#"001"#,
        source: r#"<?php

echo token_name(T_INCLUDE), "\n";
echo token_name(T_INCLUDE_ONCE), "\n";
echo token_name(T_EVAL), "\n";
echo token_name(T_REQUIRE), "\n";
echo token_name(T_REQUIRE_ONCE), "\n";
echo token_name(T_LOGICAL_OR), "\n";
echo token_name(T_LOGICAL_XOR), "\n";
echo token_name(T_LOGICAL_AND), "\n";
echo token_name(T_PRINT), "\n";
echo token_name(T_PLUS_EQUAL), "\n";
echo token_name(T_MINUS_EQUAL), "\n";
echo token_name(T_MUL_EQUAL), "\n";
echo token_name(T_DIV_EQUAL), "\n";
echo token_name(T_CONCAT_EQUAL), "\n";
echo token_name(T_MOD_EQUAL), "\n";
echo token_name(T_AND_EQUAL), "\n";
echo token_name(T_OR_EQUAL), "\n";
echo token_name(T_XOR_EQUAL), "\n";
echo token_name(T_SL_EQUAL), "\n";
echo token_name(T_SR_EQUAL), "\n";
echo token_name(T_BOOLEAN_OR), "\n";
echo token_name(T_BOOLEAN_AND), "\n";
echo token_name(T_IS_EQUAL), "\n";
echo token_name(T_IS_NOT_EQUAL), "\n";
echo token_name(T_IS_IDENTICAL), "\n";
echo token_name(T_IS_NOT_IDENTICAL), "\n";
echo token_name(T_IS_SMALLER_OR_EQUAL), "\n";
echo token_name(T_IS_GREATER_OR_EQUAL), "\n";
echo token_name(T_SL), "\n";
echo token_name(T_SR), "\n";
echo token_name(T_INC), "\n";
echo token_name(T_DEC), "\n";
echo token_name(T_INT_CAST), "\n";
echo token_name(T_DOUBLE_CAST), "\n";
echo token_name(T_STRING_CAST), "\n";
echo token_name(T_ARRAY_CAST), "\n";
echo token_name(T_OBJECT_CAST), "\n";
echo token_name(T_BOOL_CAST), "\n";
echo token_name(T_UNSET_CAST), "\n";
echo token_name(T_NEW), "\n";
echo token_name(T_EXIT), "\n";
echo token_name(T_IF), "\n";
echo token_name(T_ELSEIF), "\n";
echo token_name(T_ELSE), "\n";
echo token_name(T_ENDIF), "\n";
echo token_name(T_LNUMBER), "\n";
echo token_name(T_DNUMBER), "\n";
echo token_name(T_STRING), "\n";
echo token_name(T_STRING_VARNAME), "\n";
echo token_name(T_VARIABLE), "\n";
echo token_name(T_NUM_STRING), "\n";
echo token_name(T_INLINE_HTML), "\n";
echo token_name(T_ENCAPSED_AND_WHITESPACE), "\n";
echo token_name(T_CONSTANT_ENCAPSED_STRING), "\n";
echo token_name(T_ECHO), "\n";
echo token_name(T_DO), "\n";
echo token_name(T_WHILE), "\n";
echo token_name(T_ENDWHILE), "\n";
echo token_name(T_FOR), "\n";
echo token_name(T_ENDFOR), "\n";
echo token_name(T_FOREACH), "\n";
echo token_name(T_ENDFOREACH), "\n";
echo token_name(T_DECLARE), "\n";
echo token_name(T_ENDDECLARE), "\n";
echo token_name(T_AS), "\n";
echo token_name(T_SWITCH), "\n";
echo token_name(T_ENDSWITCH), "\n";
echo token_name(T_CASE), "\n";
echo token_name(T_DEFAULT), "\n";
echo token_name(T_BREAK), "\n";
echo token_name(T_CONTINUE), "\n";
echo token_name(T_FUNCTION), "\n";
echo token_name(T_CONST), "\n";
echo token_name(T_RETURN), "\n";
echo token_name(T_USE), "\n";
echo token_name(T_GLOBAL), "\n";
echo token_name(T_STATIC), "\n";
echo token_name(T_VAR), "\n";
echo token_name(T_UNSET), "\n";
echo token_name(T_ISSET), "\n";
echo token_name(T_EMPTY), "\n";
echo token_name(T_CLASS), "\n";
echo token_name(T_EXTENDS), "\n";
echo token_name(T_INTERFACE), "\n";
echo token_name(T_IMPLEMENTS), "\n";
echo token_name(T_OBJECT_OPERATOR), "\n";
echo token_name(T_DOUBLE_ARROW), "\n";
echo token_name(T_LIST), "\n";
echo token_name(T_ARRAY), "\n";
echo token_name(T_CLASS_C), "\n";
echo token_name(T_FUNC_C), "\n";
echo token_name(T_PROPERTY_C), "\n";
echo token_name(T_METHOD_C), "\n";
echo token_name(T_LINE), "\n";
echo token_name(T_FILE), "\n";
echo token_name(T_COMMENT), "\n";
echo token_name(T_DOC_COMMENT), "\n";
echo token_name(T_OPEN_TAG), "\n";
echo token_name(T_OPEN_TAG_WITH_ECHO), "\n";
echo token_name(T_CLOSE_TAG), "\n";
echo token_name(T_WHITESPACE), "\n";
echo token_name(T_START_HEREDOC), "\n";
echo token_name(T_END_HEREDOC), "\n";
echo token_name(T_DOLLAR_OPEN_CURLY_BRACES), "\n";
echo token_name(T_CURLY_OPEN), "\n";
echo token_name(T_PAAMAYIM_NEKUDOTAYIM), "\n";
echo token_name(T_PAAMAYIM_NEKUDOTAYIM), "\n";
echo token_name(T_ABSTRACT), "\n";
echo token_name(T_CATCH), "\n";
echo token_name(T_FINAL), "\n";
echo token_name(T_INSTANCEOF), "\n";
echo token_name(T_PRIVATE), "\n";
echo token_name(T_PROTECTED), "\n";
echo token_name(T_PUBLIC), "\n";
echo token_name(T_THROW), "\n";
echo token_name(T_TRY), "\n";
echo token_name(T_CLONE), "\n";
echo token_name(T_HALT_COMPILER), "\n";

echo token_name(-1), "\n";
echo token_name(0x8000000F), "\n";

echo "Done\n";;
"#,
        expected: r#"T_INCLUDE
T_INCLUDE_ONCE
T_EVAL
T_REQUIRE
T_REQUIRE_ONCE
T_LOGICAL_OR
T_LOGICAL_XOR
T_LOGICAL_AND
T_PRINT
T_PLUS_EQUAL
T_MINUS_EQUAL
T_MUL_EQUAL
T_DIV_EQUAL
T_CONCAT_EQUAL
T_MOD_EQUAL
T_AND_EQUAL
T_OR_EQUAL
T_XOR_EQUAL
T_SL_EQUAL
T_SR_EQUAL
T_BOOLEAN_OR
T_BOOLEAN_AND
T_IS_EQUAL
T_IS_NOT_EQUAL
T_IS_IDENTICAL
T_IS_NOT_IDENTICAL
T_IS_SMALLER_OR_EQUAL
T_IS_GREATER_OR_EQUAL
T_SL
T_SR
T_INC
T_DEC
T_INT_CAST
T_DOUBLE_CAST
T_STRING_CAST
T_ARRAY_CAST
T_OBJECT_CAST
T_BOOL_CAST
T_UNSET_CAST
T_NEW
T_EXIT
T_IF
T_ELSEIF
T_ELSE
T_ENDIF
T_LNUMBER
T_DNUMBER
T_STRING
T_STRING_VARNAME
T_VARIABLE
T_NUM_STRING
T_INLINE_HTML
T_ENCAPSED_AND_WHITESPACE
T_CONSTANT_ENCAPSED_STRING
T_ECHO
T_DO
T_WHILE
T_ENDWHILE
T_FOR
T_ENDFOR
T_FOREACH
T_ENDFOREACH
T_DECLARE
T_ENDDECLARE
T_AS
T_SWITCH
T_ENDSWITCH
T_CASE
T_DEFAULT
T_BREAK
T_CONTINUE
T_FUNCTION
T_CONST
T_RETURN
T_USE
T_GLOBAL
T_STATIC
T_VAR
T_UNSET
T_ISSET
T_EMPTY
T_CLASS
T_EXTENDS
T_INTERFACE
T_IMPLEMENTS
T_OBJECT_OPERATOR
T_DOUBLE_ARROW
T_LIST
T_ARRAY
T_CLASS_C
T_FUNC_C
T_PROPERTY_C
T_METHOD_C
T_LINE
T_FILE
T_COMMENT
T_DOC_COMMENT
T_OPEN_TAG
T_OPEN_TAG_WITH_ECHO
T_CLOSE_TAG
T_WHITESPACE
T_START_HEREDOC
T_END_HEREDOC
T_DOLLAR_OPEN_CURLY_BRACES
T_CURLY_OPEN
T_DOUBLE_COLON
T_DOUBLE_COLON
T_ABSTRACT
T_CATCH
T_FINAL
T_INSTANCEOF
T_PRIVATE
T_PROTECTED
T_PUBLIC
T_THROW
T_TRY
T_CLONE
T_HALT_COMPILER
UNKNOWN
UNKNOWN
Done"#,
        expectf: false,
    },
    Case {
        name: r#"002"#,
        source: r#"<?php

$strings = array(
    '<?php echo 1; if (isset($a)) print $a+1; $a++; $a--; $a == 2; $a === 2; endif; ?>',
    '<?php switch($a) { case 1: break; default: break; } while($a) { exit; } ?>',
    '<?php /* comment */ if (1 || 2) { } $a = 2 | 1; $b = 3^2; $c = 4&2; ?>',
    /* feel free to add more yourself */
    'wrong syntax here'
);

foreach ($strings as $s) {
    var_dump(token_get_all($s));
}

echo "Done\n";;
"#,
        expected: r#"array(48) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [4]=>
  string(1) ";"
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(1)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [8]=>
  string(1) "("
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "isset"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) "("
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [12]=>
  string(1) ")"
  [13]=>
  string(1) ")"
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "print"
    [2]=>
    int(1)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [18]=>
  string(1) "+"
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [20]=>
  string(1) ";"
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "++"
    [2]=>
    int(1)
  }
  [24]=>
  string(1) ";"
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "--"
    [2]=>
    int(1)
  }
  [28]=>
  string(1) ";"
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "=="
    [2]=>
    int(1)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [35]=>
  string(1) ";"
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "==="
    [2]=>
    int(1)
  }
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [42]=>
  string(1) ";"
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [44]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "endif"
    [2]=>
    int(1)
  }
  [45]=>
  string(1) ";"
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [47]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
array(37) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "switch"
    [2]=>
    int(1)
  }
  [2]=>
  string(1) "("
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [4]=>
  string(1) ")"
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [6]=>
  string(1) "{"
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "case"
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [11]=>
  string(1) ":"
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "break"
    [2]=>
    int(1)
  }
  [14]=>
  string(1) ";"
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "default"
    [2]=>
    int(1)
  }
  [17]=>
  string(1) ":"
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "break"
    [2]=>
    int(1)
  }
  [20]=>
  string(1) ";"
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [22]=>
  string(1) "}"
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "while"
    [2]=>
    int(1)
  }
  [25]=>
  string(1) "("
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [27]=>
  string(1) ")"
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [29]=>
  string(1) "{"
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "exit"
    [2]=>
    int(1)
  }
  [32]=>
  string(1) ";"
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [34]=>
  string(1) "}"
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
array(47) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(13) "/* comment */"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(1)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  string(1) "("
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "||"
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [11]=>
  string(1) ")"
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [13]=>
  string(1) "{"
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [15]=>
  string(1) "}"
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [19]=>
  string(1) "="
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [23]=>
  string(1) "|"
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [26]=>
  string(1) ";"
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [30]=>
  string(1) "="
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "3"
    [2]=>
    int(1)
  }
  [33]=>
  string(1) "^"
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [35]=>
  string(1) ";"
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(1)
  }
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [39]=>
  string(1) "="
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "4"
    [2]=>
    int(1)
  }
  [42]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "&"
    [2]=>
    int(1)
  }
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [44]=>
  string(1) ";"
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
array(1) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(17) "wrong syntax here"
    [2]=>
    int(1)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"003"#,
        source: r#"<?php

var_dump(token_get_all(""));
var_dump(token_get_all(0));
var_dump(token_get_all(-1));

echo "Done\n";;
"#,
        expected: r#"array(0) {
}
array(1) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "0"
    [2]=>
    int(1)
  }
}
array(1) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "-1"
    [2]=>
    int(1)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"PhpToken_constructor"#,
        source: r#"<?php

$token = new PhpToken(300, 'function');
var_dump($token);
$token = new PhpToken(300, 'function', 10);
var_dump($token);
$token = new PhpToken(300, 'function', 10, 100);
var_dump($token);;
"#,
        expected: r#"object(PhpToken)#1 (4) {
  ["id"]=>
  int(300)
  ["text"]=>
  string(8) "function"
  ["line"]=>
  int(-1)
  ["pos"]=>
  int(-1)
}
object(PhpToken)#2 (4) {
  ["id"]=>
  int(300)
  ["text"]=>
  string(8) "function"
  ["line"]=>
  int(10)
  ["pos"]=>
  int(-1)
}
object(PhpToken)#1 (4) {
  ["id"]=>
  int(300)
  ["text"]=>
  string(8) "function"
  ["line"]=>
  int(10)
  ["pos"]=>
  int(100)
}"#,
        expectf: false,
    },
    Case {
        name: r#"PhpToken_extension"#,
        source: r#"<?php

$code = <<<'PHP'
<?PHP
FUNCTION FOO() {
    ECHO "bar";
}
PHP;

class MyPhpToken extends PhpToken {
    public int $extra = 123;

    public function getLoweredText(): string {
        return strtolower($this->text);
    }
}

foreach (MyPhpToken::tokenize($code) as $token) {
    echo $token->getLoweredText();

    if ($token->extra !== 123) {
        echo "Missing property!\n";
    }
};
"#,
        expected: r#"<?php
function foo() {
    echo "bar";
}"#,
        expectf: false,
    },
    Case {
        name: r#"PhpToken_extension_errors"#,
        source: r#"<?php

class MyPhpToken1 extends PhpToken {
    public $extra = UNKNOWN;
}

try {
    var_dump(MyPhpToken1::tokenize("<?php foo"));
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

abstract class MyPhpToken2 extends PhpToken {
}

try {
    var_dump(MyPhpToken2::tokenize("<?php foo"));
} catch (Error $e) {
    echo $e->getMessage(), "\n";
};
"#,
        expected: r#"Undefined constant "UNKNOWN"
Cannot instantiate abstract class MyPhpToken2"#,
        expectf: false,
    },
    Case {
        name: r#"PhpToken_final_constructor"#,
        source: r#"<?php

class MyPhpToken extends PhpToken {
    public function __construct() {
    }
};
"#,
        expected: r#"Fatal error: Cannot override final method PhpToken::__construct() in %s on line %d"#,
        expectf: true,
    },
    Case {
        name: r#"PhpToken_methods"#,
        source: r#"<?php

$code = <<<'PHP'
<?php
// comment
/** comment */
function foo() {
    echo "bar";
}
PHP;

// Token names and ignorability.
$tokens = PhpToken::tokenize($code);
foreach ($tokens as $i => $token) {
    printf("[%2d] %-26s %s\n", $i, $token->getTokenName(),
        $token->isIgnorable() ? "ignorable" : "meaningful");
}

// is() variations
$token = $tokens[5];

echo "\nSuccess:\n";
var_dump($token->is(T_FUNCTION));
var_dump($token->is('function'));
var_dump($token->is(['class', T_FUNCTION]));
var_dump($token->is([T_CLASS, 'function']));

echo "\nFailure:\n";
var_dump($token->is(T_CLASS));
var_dump($token->is('class'));
var_dump($token->is(['class', T_TRAIT]));
var_dump($token->is([T_CLASS, 'trait']));

echo "\nError:\n";
try {
    $token->is(3.141);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $token->is([3.141]);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

unset($token->id);
unset($token->text);
try {
    $token->is(T_FUNCTION);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    $token->is('function');
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    $token->is([T_FUNCTION]);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    $token->is(['function']);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

echo "\nName of unknown token:\n";
$token = new PhpToken(100000, "foo");
var_dump($token->getTokenName());;
"#,
        expected: r#"[ 0] T_OPEN_TAG                 ignorable
[ 1] T_COMMENT                  ignorable
[ 2] T_WHITESPACE               ignorable
[ 3] T_DOC_COMMENT              ignorable
[ 4] T_WHITESPACE               ignorable
[ 5] T_FUNCTION                 meaningful
[ 6] T_WHITESPACE               ignorable
[ 7] T_STRING                   meaningful
[ 8] (                          meaningful
[ 9] )                          meaningful
[10] T_WHITESPACE               ignorable
[11] {                          meaningful
[12] T_WHITESPACE               ignorable
[13] T_ECHO                     meaningful
[14] T_WHITESPACE               ignorable
[15] T_CONSTANT_ENCAPSED_STRING meaningful
[16] ;                          meaningful
[17] T_WHITESPACE               ignorable
[18] }                          meaningful

Success:
bool(true)
bool(true)
bool(true)
bool(true)

Failure:
bool(false)
bool(false)
bool(false)
bool(false)

Error:
PhpToken::is(): Argument #1 ($kind) must be of type string|int|array, float given
PhpToken::is(): Argument #1 ($kind) must only have elements of type string|int, float given
Typed property PhpToken::$id must not be accessed before initialization
Typed property PhpToken::$text must not be accessed before initialization
Typed property PhpToken::$id must not be accessed before initialization
Typed property PhpToken::$text must not be accessed before initialization

Name of unknown token:
NULL"#,
        expectf: false,
    },
    Case {
        name: r#"PhpToken_toString"#,
        source: r#"<?php

$tokens = PhpToken::tokenize('<?php echo "Hello ". $what;');
var_dump(implode($tokens));

var_dump($tokens[0] instanceof Stringable);
var_dump((string) $tokens[0]);
var_dump($tokens[0]->__toString());;
"#,
        expected: r#"string(27) "<?php echo "Hello ". $what;"
bool(true)
string(6) "<?php "
string(6) "<?php ""#,
        expectf: false,
    },
    Case {
        name: r#"PhpToken_tokenize"#,
        source: r#"<?php

$code = <<<'PHP'
<?php
function foo() {
    echo "bar";
}
PHP;
var_dump(PhpToken::tokenize($code));
var_dump(PhpToken::tokenize($code, TOKEN_PARSE));;
"#,
        expected: r#"array(15) {
  [0]=>
  object(PhpToken)#1 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(6) "<?php
"
    ["line"]=>
    int(1)
    ["pos"]=>
    int(0)
  }
  [1]=>
  object(PhpToken)#2 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(8) "function"
    ["line"]=>
    int(2)
    ["pos"]=>
    int(6)
  }
  [2]=>
  object(PhpToken)#3 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) " "
    ["line"]=>
    int(2)
    ["pos"]=>
    int(14)
  }
  [3]=>
  object(PhpToken)#4 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(3) "foo"
    ["line"]=>
    int(2)
    ["pos"]=>
    int(15)
  }
  [4]=>
  object(PhpToken)#5 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) "("
    ["line"]=>
    int(2)
    ["pos"]=>
    int(18)
  }
  [5]=>
  object(PhpToken)#6 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) ")"
    ["line"]=>
    int(2)
    ["pos"]=>
    int(19)
  }
  [6]=>
  object(PhpToken)#7 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) " "
    ["line"]=>
    int(2)
    ["pos"]=>
    int(20)
  }
  [7]=>
  object(PhpToken)#8 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) "{"
    ["line"]=>
    int(2)
    ["pos"]=>
    int(21)
  }
  [8]=>
  object(PhpToken)#9 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(5) "
    "
    ["line"]=>
    int(2)
    ["pos"]=>
    int(22)
  }
  [9]=>
  object(PhpToken)#10 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(4) "echo"
    ["line"]=>
    int(3)
    ["pos"]=>
    int(27)
  }
  [10]=>
  object(PhpToken)#11 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) " "
    ["line"]=>
    int(3)
    ["pos"]=>
    int(31)
  }
  [11]=>
  object(PhpToken)#12 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(5) ""bar""
    ["line"]=>
    int(3)
    ["pos"]=>
    int(32)
  }
  [12]=>
  object(PhpToken)#13 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) ";"
    ["line"]=>
    int(3)
    ["pos"]=>
    int(37)
  }
  [13]=>
  object(PhpToken)#14 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) "
"
    ["line"]=>
    int(3)
    ["pos"]=>
    int(38)
  }
  [14]=>
  object(PhpToken)#15 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) "}"
    ["line"]=>
    int(4)
    ["pos"]=>
    int(39)
  }
}
array(15) {
  [0]=>
  object(PhpToken)#15 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(6) "<?php
"
    ["line"]=>
    int(1)
    ["pos"]=>
    int(0)
  }
  [1]=>
  object(PhpToken)#14 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(8) "function"
    ["line"]=>
    int(2)
    ["pos"]=>
    int(6)
  }
  [2]=>
  object(PhpToken)#13 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) " "
    ["line"]=>
    int(2)
    ["pos"]=>
    int(14)
  }
  [3]=>
  object(PhpToken)#12 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(3) "foo"
    ["line"]=>
    int(2)
    ["pos"]=>
    int(15)
  }
  [4]=>
  object(PhpToken)#11 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) "("
    ["line"]=>
    int(2)
    ["pos"]=>
    int(18)
  }
  [5]=>
  object(PhpToken)#10 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) ")"
    ["line"]=>
    int(2)
    ["pos"]=>
    int(19)
  }
  [6]=>
  object(PhpToken)#9 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) " "
    ["line"]=>
    int(2)
    ["pos"]=>
    int(20)
  }
  [7]=>
  object(PhpToken)#8 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) "{"
    ["line"]=>
    int(2)
    ["pos"]=>
    int(21)
  }
  [8]=>
  object(PhpToken)#7 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(5) "
    "
    ["line"]=>
    int(2)
    ["pos"]=>
    int(22)
  }
  [9]=>
  object(PhpToken)#6 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(4) "echo"
    ["line"]=>
    int(3)
    ["pos"]=>
    int(27)
  }
  [10]=>
  object(PhpToken)#5 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) " "
    ["line"]=>
    int(3)
    ["pos"]=>
    int(31)
  }
  [11]=>
  object(PhpToken)#4 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(5) ""bar""
    ["line"]=>
    int(3)
    ["pos"]=>
    int(32)
  }
  [12]=>
  object(PhpToken)#3 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) ";"
    ["line"]=>
    int(3)
    ["pos"]=>
    int(37)
  }
  [13]=>
  object(PhpToken)#2 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) "
"
    ["line"]=>
    int(3)
    ["pos"]=>
    int(38)
  }
  [14]=>
  object(PhpToken)#1 (4) {
    ["id"]=>
    int(%d)
    ["text"]=>
    string(1) "}"
    ["line"]=>
    int(4)
    ["pos"]=>
    int(39)
  }
}"#,
        expectf: true,
    },
    Case {
        name: r#"attributes"#,
        source: r#"<?php

$tokens = token_get_all('<?php #[A1(1, 2)] class C1 { }');

$attr = $tokens[1];
var_dump(token_name(T_ATTRIBUTE));
var_dump($attr[0] === T_ATTRIBUTE);
var_dump($attr[1]);

$class = $tokens[2];
var_dump($class[1]);;
"#,
        expected: r##"string(11) "T_ATTRIBUTE"
bool(true)
string(2) "#["
string(2) "A1""##,
        expectf: false,
    },
    Case {
        name: r#"bad_character"#,
        source: r#"<?php

$codes = [
    "<?php \0 foo",
    "<?php \1 bar",
    "<?php \1\2 bar \3",
];

foreach ($codes as $code) {
    foreach (token_get_all($code) as $token) {
        if (is_array($token)) {
            echo token_name($token[0]), " ", strlen($token[1]), "\n";
        } else {
            echo $token, "\n";
        }
    }
    echo "\n";
};
"#,
        expected: r#"T_OPEN_TAG 6
T_BAD_CHARACTER 1
T_WHITESPACE 1
T_STRING 3

T_OPEN_TAG 6
T_BAD_CHARACTER 1
T_WHITESPACE 1
T_STRING 3

T_OPEN_TAG 6
T_BAD_CHARACTER 1
T_BAD_CHARACTER 1
T_WHITESPACE 1
T_STRING 3
T_WHITESPACE 1
T_BAD_CHARACTER 1"#,
        expectf: false,
    },
    Case {
        name: r#"bug26463"#,
        source: r#"<?php
$str = '<?php
$x=<<<DD
jhdsjkfhjdsh
DD
."";
$a=<<<DDDD
jhdsjkfhjdsh
DDDD;
?>';
var_dump(token_get_all($str));;
"#,
        expected: r#"array(19) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$x"
    [2]=>
    int(2)
  }
  [2]=>
  string(1) "="
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<<<DD
"
    [2]=>
    int(2)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(13) "jhdsjkfhjdsh
"
    [2]=>
    int(3)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "DD"
    [2]=>
    int(4)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [7]=>
  string(1) "."
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) """"
    [2]=>
    int(5)
  }
  [9]=>
  string(1) ";"
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(6)
  }
  [12]=>
  string(1) "="
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "<<<DDDD
"
    [2]=>
    int(6)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(13) "jhdsjkfhjdsh
"
    [2]=>
    int(7)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "DDDD"
    [2]=>
    int(8)
  }
  [16]=>
  string(1) ";"
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(8)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(9)
  }
}"#,
        expectf: true,
    },
    Case {
        name: r#"bug54089"#,
        source: r#"<?php
$codes = array(
    "<?php __halt_compiler",
    "<?php __halt_compiler(",
    "<?php __halt_compiler();",
    "<?php __halt_compiler();ABC",
    "<?php __halt_compiler\n(\n)\n;ABC",
    "<?php __halt_compiler\nabc\ndef\nghi ABC",
);
foreach ($codes as $code) {
    $tokens = token_get_all($code);
    var_dump($tokens);

    $code = '';
    foreach ($tokens as $t)
    {
        $code .= isset($t[1]) ? $t[1] : $t;
    }
    var_dump($code);
};
"#,
        expected: r#"array(2) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(15) "__halt_compiler"
    [2]=>
    int(1)
  }
}
string(21) "<?php __halt_compiler"
array(3) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(15) "__halt_compiler"
    [2]=>
    int(1)
  }
  [2]=>
  string(1) "("
}
string(22) "<?php __halt_compiler("
array(5) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(15) "__halt_compiler"
    [2]=>
    int(1)
  }
  [2]=>
  string(1) "("
  [3]=>
  string(1) ")"
  [4]=>
  string(1) ";"
}
string(24) "<?php __halt_compiler();"
array(6) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(15) "__halt_compiler"
    [2]=>
    int(1)
  }
  [2]=>
  string(1) "("
  [3]=>
  string(1) ")"
  [4]=>
  string(1) ";"
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "ABC"
    [2]=>
    int(1)
  }
}
string(27) "<?php __halt_compiler();ABC"
array(9) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(15) "__halt_compiler"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "("
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [5]=>
  string(1) ")"
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(3)
  }
  [7]=>
  string(1) ";"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "ABC"
    [2]=>
    int(4)
  }
}
string(30) "<?php __halt_compiler
(
)
;ABC"
array(9) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(15) "__halt_compiler"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(1)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "abc"
    [2]=>
    int(2)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "def"
    [2]=>
    int(3)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(3)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "ghi"
    [2]=>
    int(4)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) " ABC"
    [2]=>
    int(4)
  }
}
string(37) "<?php __halt_compiler
abc
def
ghi ABC""#,
        expectf: true,
    },
    Case {
        name: r#"bug60097"#,
        source: r#"<?php

var_dump(token_get_all('<?php
<<<DOC1
{$s(<<<DOC2
DOC2
)}
DOC1;
'));;
"#,
        expected: r#"array(14) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "<<<DOC1
"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "{"
    [2]=>
    int(3)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$s"
    [2]=>
    int(3)
  }
  [4]=>
  string(1) "("
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "<<<DOC2
"
    [2]=>
    int(3)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "DOC2"
    [2]=>
    int(4)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [8]=>
  string(1) ")"
  [9]=>
  string(1) "}"
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "DOC1"
    [2]=>
    int(6)
  }
  [12]=>
  string(1) ";"
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
}"#,
        expectf: true,
    },
    Case {
        name: r#"bug67395"#,
        source: r#"<?php

$powToken = token_get_all('<?php **')[1][0];
var_dump(token_name($powToken));

$powEqualToken = token_get_all('<?php **=')[1][0];
var_dump(token_name($powEqualToken));;
"#,
        expected: r#"string(5) "T_POW"
string(11) "T_POW_EQUAL""#,
        expectf: false,
    },
    Case {
        name: r#"bug76437"#,
        source: r#"<?php
$tests = [
    ['<?=$a?>', 0],
    ['<?php echo 2; ?>', 6],
    ["<?php echo 2; ?>\n", 6],
];
foreach ($tests as [$code, $index]) {
    $open_tag1 = token_get_all($code)[$index];
    $open_tag2 = token_get_all($code, TOKEN_PARSE)[$index];
    echo token_name($open_tag1[0]), ": \"$open_tag1[1]\" on line $open_tag1[2]\n";
    var_dump($open_tag1 === $open_tag2);
};
"#,
        expected: r#"T_OPEN_TAG_WITH_ECHO: "<?=" on line 1
bool(true)
T_CLOSE_TAG: "?>" on line 1
bool(true)
T_CLOSE_TAG: "?>
" on line 1
bool(true)"#,
        expectf: false,
    },
    Case {
        name: r#"bug76991"#,
        source: r#"<?php

$code = <<<CODE
<?php
<<<TEST
\$a
 TEST;

<<<END
\$a
 END;
CODE;
foreach (token_get_all($code) as $token) {
    if (is_array($token)) {
        echo token_name($token[0]) . ": " .str_replace("\n", '\n', $token[1]);
    } else {
        echo $token;
    }
    echo "\n";
};
"#,
        expected: r#"T_OPEN_TAG: <?php\n
T_START_HEREDOC: <<<TEST\n
T_VARIABLE: $a
T_ENCAPSED_AND_WHITESPACE: \n
T_END_HEREDOC:  TEST
;
T_WHITESPACE: \n\n
T_START_HEREDOC: <<<END\n
T_VARIABLE: $a
T_ENCAPSED_AND_WHITESPACE: \n
T_END_HEREDOC:  END
;"#,
        expectf: false,
    },
    Case {
        name: r#"bug77966"#,
        source: r#"<?php

$code = <<<'CODE'
<?php
class C {
    use A {
        namespace as bar;
    }
}
CODE;

$tokens = PhpToken::tokenize($code, TOKEN_PARSE);
foreach ($tokens as $token) {
    echo "{$token->getTokenName()}: \"$token->text\"\n";
};
"#,
        expected: r#"T_OPEN_TAG: "<?php
"
T_CLASS: "class"
T_WHITESPACE: " "
T_STRING: "C"
T_WHITESPACE: " "
{: "{"
T_WHITESPACE: "
    "
T_USE: "use"
T_WHITESPACE: " "
T_STRING: "A"
T_WHITESPACE: " "
{: "{"
T_WHITESPACE: "
        "
T_STRING: "namespace"
T_WHITESPACE: " "
T_AS: "as"
T_WHITESPACE: " "
T_STRING: "bar"
;: ";"
T_WHITESPACE: "
    "
}: "}"
T_WHITESPACE: "
"
}: "}""#,
        expectf: false,
    },
    Case {
        name: r#"bug80462"#,
        source: r#"<?php

foreach (PhpToken::tokenize('<?php $foo = $a?->b();', TOKEN_PARSE) as $token) {
    echo $token->getTokenName(), "\n";
};
"#,
        expected: r#"T_OPEN_TAG
T_VARIABLE
T_WHITESPACE
=
T_WHITESPACE
T_VARIABLE
T_NULLSAFE_OBJECT_OPERATOR
T_STRING
(
)
;"#,
        expectf: false,
    },
    Case {
        name: r#"bug81342"#,
        source: r#"<?php

$tokens = PhpToken::tokenize('<?php $x & $x; $x &
    $baz;
');
foreach ($tokens as $token) {
    echo $token->getTokenName(), "\n";
};
"#,
        expected: r#"T_OPEN_TAG
T_VARIABLE
T_WHITESPACE
T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
T_WHITESPACE
T_VARIABLE
;
T_WHITESPACE
T_VARIABLE
T_WHITESPACE
T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
T_WHITESPACE
T_VARIABLE
;
T_WHITESPACE"#,
        expectf: false,
    },
    Case {
        name: r#"gh19507_eval"#,
        source: r#"<?php

set_error_handler(function (int $errno, string $msg) {
    eval('123;');
    echo 'error handler called: ', $msg, "\n";
});


$tokens = PhpToken::tokenize('<?php (double) $x;', TOKEN_PARSE);
foreach ($tokens as $token) {
    echo $token->getTokenName(), "\n";
};
"#,
        expected: r#"error handler called: Non-canonical cast (double) is deprecated, use the (float) cast instead
T_OPEN_TAG
T_DOUBLE_CAST
T_WHITESPACE
T_VARIABLE
;"#,
        expectf: false,
    },
    Case {
        name: r#"gh19507_throw"#,
        source: r#"<?php

set_error_handler(function (int $errno, string $msg) {
    throw new RuntimeException('error handler called: ' . $msg);
});


$tokens = PhpToken::tokenize('<?php (double) $x;', TOKEN_PARSE);
foreach ($tokens as $token) {
    echo $token->getTokenName(), "\n";
};
"#,
        expected: r#"Fatal error: Uncaught RuntimeException: error handler called: Non-canonical cast (double) is deprecated, use the (float) cast instead in %s:%d
Stack trace:
#0 [internal function]: {closure:%s:%d}(%d, 'Non-canonical c...', '', 1)
#1 %s(%d): PhpToken::tokenize('<?php (double) ...', %d)
#2 {main}
  thrown in %s on line %d"#,
        expectf: true,
    },
    Case {
        name: r#"invalid_large_octal_with_underscores"#,
        source: r#"<?php

var_dump(token_get_all("<?php 0_10000000000000000000009;"));;
"#,
        expected: r#"array(3) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(25) "0_10000000000000000000009"
    [2]=>
    int(1)
  }
  [2]=>
  string(1) ";"
}"#,
        expectf: true,
    },
    Case {
        name: r#"invalid_octal_dnumber"#,
        source: r#"<?php
echo token_name(token_get_all('<?php 0177777777777777777777787')[1][0]), "\n";;
"#,
        expected: r#"T_DNUMBER"#,
        expectf: false,
    },
    Case {
        name: r#"namespaced_names"#,
        source: r#"<?php

$code = <<<'CODE'
<?php
Foo
Foo\Bar
\Foo\Bar
namespace\Foo
Foo \ Bar
CODE;

foreach (PhpToken::tokenize($code) as $token) {
    echo "{$token->getTokenName()}: \"$token->text\"\n";
};
"#,
        expected: r#"T_OPEN_TAG: "<?php
"
T_STRING: "Foo"
T_WHITESPACE: "
"
T_NAME_QUALIFIED: "Foo\Bar"
T_WHITESPACE: "
"
T_NAME_FULLY_QUALIFIED: "\Foo\Bar"
T_WHITESPACE: "
"
T_NAME_RELATIVE: "namespace\Foo"
T_WHITESPACE: "
"
T_STRING: "Foo"
T_WHITESPACE: " "
T_NS_SEPARATOR: "\"
T_WHITESPACE: " "
T_STRING: "Bar""#,
        expectf: false,
    },
    Case {
        name: r#"no_inline_html_split"#,
        source: r#"<?php

var_dump(token_get_all(<<<'PHP'
Foo<?phpBar
PHP));;
"#,
        expected: r#"array(1) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(11) "Foo<?phpBar"
    [2]=>
    int(1)
  }
}"#,
        expectf: true,
    },
    Case {
        name: r#"parse_errors"#,
        source: r#"<?php

function test_parse_error($code) {
    try {
        var_dump(token_get_all($code, TOKEN_PARSE));
    } catch (ParseError $e) {
        echo $e->getMessage(), "\n";
    }

    foreach (token_get_all($code) as $token) {
        if (is_array($token)) {
            echo token_name($token[0]), " ($token[1])\n";
        } else {
            echo "$token\n";
        }
    }
    echo "\n";
}

test_parse_error('<?php var_dump(078);');
test_parse_error('<?php var_dump("\u{xyz}");');
test_parse_error('<?php var_dump("\u{ffffff}");');
test_parse_error('<?php var_dump(078 + 078);');;
"#,
        expected: r#"Invalid numeric literal
T_OPEN_TAG (<?php )
T_STRING (var_dump)
(
T_LNUMBER (078)
)
;

Invalid UTF-8 codepoint escape sequence
T_OPEN_TAG (<?php )
T_STRING (var_dump)
(
T_CONSTANT_ENCAPSED_STRING ("\u{xyz}")
)
;

Invalid UTF-8 codepoint escape sequence: Codepoint too large
T_OPEN_TAG (<?php )
T_STRING (var_dump)
(
T_CONSTANT_ENCAPSED_STRING ("\u{ffffff}")
)
;

Invalid numeric literal
T_OPEN_TAG (<?php )
T_STRING (var_dump)
(
T_LNUMBER (078)
T_WHITESPACE ( )
+
T_WHITESPACE ( )
T_LNUMBER (078)
)
;"#,
        expectf: false,
    },
    Case {
        name: r#"php_tag_only"#,
        source: r#"<?php

foreach (token_get_all("<?php") as $token) {
    echo token_name($token[0]), "\n";
}
echo "\n";
foreach (token_get_all("Foobar<?php") as $token) {
    echo token_name($token[0]), "\n";
};
"#,
        expected: r#"T_OPEN_TAG

T_INLINE_HTML
T_OPEN_TAG"#,
        expectf: false,
    },
    Case {
        name: r#"php_tag_only_2"#,
        source: r#"<?php

foreach (token_get_all("<?php") as $token) {
    echo token_name($token[0]), "\n";
}
echo "\n";
foreach (token_get_all("Foobar<?php") as $token) {
    echo token_name($token[0]), "\n";
};
"#,
        expected: r#"T_OPEN_TAG

T_INLINE_HTML
T_OPEN_TAG"#,
        expectf: false,
    },
    Case {
        name: r#"token_get_all_TOKEN_PARSE_000"#,
        source: r#"<?php

try {
    token_get_all('<?php invalid code;', TOKEN_PARSE);
} catch (ParseError $e) {
    echo $e->getMessage(), PHP_EOL;
}

echo "Done";;
"#,
        expected: r#"syntax error, unexpected identifier "code"
Done"#,
        expectf: false,
    },
    Case {
        name: r#"token_get_all_TOKEN_PARSE_001"#,
        source: r#"<?php
$tokens = token_get_all('<?php
X::continue;
X::$continue;
$x->$continue;
X::continue();
$x->continue();
X::class;
$x->__halt_compiler();

class X {
    const CONTINUE = 1;
    public $x = self::CONTINUE + 1;
}
', TOKEN_PARSE);

array_walk($tokens, function($tk) {
  if(is_array($tk)) {
    if(($t = token_name($tk[0])) == 'T_WHITESPACE') return;
    echo "L{$tk[2]}: ".$t." {$tk[1]}", PHP_EOL;
  }
  else echo $tk, PHP_EOL;
});

echo "Done";;
"#,
        expected: r#"L1: T_OPEN_TAG <?php

L2: T_STRING X
L2: T_DOUBLE_COLON ::
L2: T_STRING continue
;
L3: T_STRING X
L3: T_DOUBLE_COLON ::
L3: T_VARIABLE $continue
;
L4: T_VARIABLE $x
L4: T_OBJECT_OPERATOR ->
L4: T_VARIABLE $continue
;
L5: T_STRING X
L5: T_DOUBLE_COLON ::
L5: T_STRING continue
(
)
;
L6: T_VARIABLE $x
L6: T_OBJECT_OPERATOR ->
L6: T_STRING continue
(
)
;
L7: T_STRING X
L7: T_DOUBLE_COLON ::
L7: T_STRING class
;
L8: T_VARIABLE $x
L8: T_OBJECT_OPERATOR ->
L8: T_STRING __halt_compiler
(
)
;
L10: T_CLASS class
L10: T_STRING X
{
L11: T_CONST const
L11: T_STRING CONTINUE
=
L11: T_LNUMBER 1
;
L12: T_PUBLIC public
L12: T_VARIABLE $x
=
L12: T_STRING self
L12: T_DOUBLE_COLON ::
L12: T_STRING CONTINUE
+
L12: T_LNUMBER 1
;
}
Done"#,
        expectf: false,
    },
    Case {
        name: r#"token_get_all_TOKEN_PARSE_002"#,
        source: r#"<?php
$tokens = token_get_all('<?php
  class SomeClass {
      const CONST = 1;
      const CONTINUE = (self::CONST + 1);
      const ARRAY = [1, self::CONTINUE => [3, 4], 5];
  }
', TOKEN_PARSE);

array_walk($tokens, function($tk) {
  if(is_array($tk)) {
    if(($t = token_name($tk[0])) == 'T_WHITESPACE') return;
    echo "L{$tk[2]}: ".$t." {$tk[1]}", PHP_EOL;
  }
  else echo $tk, PHP_EOL;
});

echo "Done";;
"#,
        expected: r#"L1: T_OPEN_TAG <?php

L2: T_CLASS class
L2: T_STRING SomeClass
{
L3: T_CONST const
L3: T_STRING CONST
=
L3: T_LNUMBER 1
;
L4: T_CONST const
L4: T_STRING CONTINUE
=
(
L4: T_STRING self
L4: T_DOUBLE_COLON ::
L4: T_STRING CONST
+
L4: T_LNUMBER 1
)
;
L5: T_CONST const
L5: T_STRING ARRAY
=
[
L5: T_LNUMBER 1
,
L5: T_STRING self
L5: T_DOUBLE_COLON ::
L5: T_STRING CONTINUE
L5: T_DOUBLE_ARROW =>
[
L5: T_LNUMBER 3
,
L5: T_LNUMBER 4
]
,
L5: T_LNUMBER 5
]
;
}
Done"#,
        expectf: false,
    },
    Case {
        name: r#"token_get_all_TOKEN_PARSE_oom_feedback"#,
        source: r#"<?php
$code = '<?php X::continue;';
ini_set('memory_limit', memory_get_usage() + 1024);
token_get_all($code, TOKEN_PARSE);;
"#,
        expected: r#"Fatal error: Allowed memory size of %d bytes exhausted %S(tried to allocate %d bytes) in %s on line %d"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_basic"#,
        source: r#"<?php
echo "*** Testing token_get_all() : basic functionality ***\n";

// with php open/close tags
$source = '<?php echo "Hello World"; ?>';
echo "-- source string with PHP open and close tags --\n";
var_dump( token_get_all($source) );

// without php open/close tags testing for T_INLINE_HTML
$source = "echo 'Hello World';";
echo "-- source string without PHP open and close tags --\n";
var_dump( token_get_all($source) );

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : basic functionality ***
-- source string with PHP open and close tags --
array(7) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(13) ""Hello World""
    [2]=>
    int(1)
  }
  [4]=>
  string(1) ";"
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- source string without PHP open and close tags --
array(1) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(19) "echo 'Hello World';"
    [2]=>
    int(1)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_heredoc_nowdoc"#,
        source: r#"<?php

function test(string $code, int $flags)
{
    try {
        $tokens = token_get_all($code, $flags);
        foreach ($tokens as $token) {
            if (is_array($token)) {
                echo "Line {$token[2]}: ", token_name($token[0]), " ('{$token[1]}')\n";
            }
        }
    } catch (ParseError $e) {
        echo "Parse error: {$e->getMessage()} on line {$e->getLine()}\n";
    }
}

$tests = [];

$tests[1] = <<<'OUTER_END'
<?php <<<INNER_END
INNER_END;
OUTER_END;

$tests[2] = <<<'OUTER_END'
<?php <<<INNER_END
  INNER_END;
OUTER_END;

$tests[3] = <<<'OUTER_END'
<?php <<<'INNER_END'
INNER_END;
OUTER_END;

$tests[4] = <<<'OUTER_END'
<?php <<<'INNER_END'
  INNER_END;
OUTER_END;

$tests[5] = <<<'OUTER_END'
  <?php <<<INNER_END
  a
  OUTER_END;

$tests[6] = <<<'OUTER_END'
<?php <<<INNER_END
ab
OUTER_END;

$tests[7] = <<<'OUTER_END'
<?php <<<INNER_END
a
INNER_END;
OUTER_END;

$tests[8] = <<<CODE
<?php
 \t<<<'DOC'
 \tXXX
 \tDOC;
CODE;

$tests[9] = <<<'OUTER_END'
<?php <<<INNER_END
ab
INNER_END;
OUTER_END;

$tests[10] = <<<'OUTER_END'
<?php <<<INNER_END
  ab
  INNER_END;
OUTER_END;

$tests[11] = <<<'OUTER_END'
<?php <<<INNER_END
abc
   INNER_END;
OUTER_END;

$tests[12] = <<<'OUTER_END'
<?php <<<INNER_END

   INNER_END;
OUTER_END;

$tests[13] = <<<'OUTER_END'
<?php <<<'INNER_END'
ab
INNER_END;
OUTER_END;

$tests[14] = <<<'OUTER_END'
<?php <<<'INNER_END'
  ab
  INNER_END;
OUTER_END;

$tests[15] = <<<'OUTER_END'
<?php <<<'INNER_END'
abc
   INNER_END;
OUTER_END;

$tests[16] = <<<'OUTER_END'
<?php <<<'INNER_END'

   INNER_END;
OUTER_END;

$tests[17] = <<<OUTER_END
<?php <<<INNER_END
ab
ab
\t
OUTER_END;

$tests[18] = <<<OUTER_END
<?php <<<'INNER_END'
ab
ab
\t
OUTER_END;

echo "Without TOKEN_PARSE:\n";
foreach ($tests as $i => $test) {
    echo "\nTest case $i\n\n";
    test($test, 0);
}

echo "\nWith TOKEN_PARSE:\n";
foreach ($tests as $i => $test) {
    echo "\nTest case $i\n\n";
    test($test, TOKEN_PARSE);
};
"#,
        expected: r#"Without TOKEN_PARSE:

Test case 1

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_END_HEREDOC ('INNER_END')

Test case 2

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_END_HEREDOC ('  INNER_END')

Test case 3

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_END_HEREDOC ('INNER_END')

Test case 4

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_END_HEREDOC ('  INNER_END')

Test case 5

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('a')

Test case 6

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('ab')

Test case 7

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('a
')
Line 3: T_END_HEREDOC ('INNER_END')

Test case 8

Line 1: T_OPEN_TAG ('<?php
')
Line 2: T_WHITESPACE (' 	')
Line 2: T_START_HEREDOC ('<<<'DOC'
')
Line 3: T_ENCAPSED_AND_WHITESPACE (' 	XXX
')
Line 4: T_END_HEREDOC (' 	DOC')

Test case 9

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('ab
')
Line 3: T_END_HEREDOC ('INNER_END')

Test case 10

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('  ab
')
Line 3: T_END_HEREDOC ('  INNER_END')

Test case 11

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('abc
')
Line 3: T_END_HEREDOC ('   INNER_END')

Test case 12

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('
')
Line 3: T_END_HEREDOC ('   INNER_END')

Test case 13

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('ab
')
Line 3: T_END_HEREDOC ('INNER_END')

Test case 14

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('  ab
')
Line 3: T_END_HEREDOC ('  INNER_END')

Test case 15

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('abc
')
Line 3: T_END_HEREDOC ('   INNER_END')

Test case 16

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('
')
Line 3: T_END_HEREDOC ('   INNER_END')

Test case 17

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('ab
ab
	')

Test case 18

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('ab
ab
	')

With TOKEN_PARSE:

Test case 1

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_END_HEREDOC ('INNER_END')

Test case 2

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_END_HEREDOC ('  INNER_END')

Test case 3

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_END_HEREDOC ('INNER_END')

Test case 4

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_END_HEREDOC ('  INNER_END')

Test case 5

Parse error: syntax error, unexpected end of file, expecting variable or heredoc end or "${" or "{$" on line 2

Test case 6

Parse error: syntax error, unexpected end of file, expecting variable or heredoc end or "${" or "{$" on line 2

Test case 7

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('a
')
Line 3: T_END_HEREDOC ('INNER_END')

Test case 8

Parse error: Invalid indentation - tabs and spaces cannot be mixed on line 3

Test case 9

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('ab
')
Line 3: T_END_HEREDOC ('INNER_END')

Test case 10

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('  ab
')
Line 3: T_END_HEREDOC ('  INNER_END')

Test case 11

Parse error: Invalid body indentation level (expecting an indentation level of at least 3) on line 2

Test case 12

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<INNER_END
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('
')
Line 3: T_END_HEREDOC ('   INNER_END')

Test case 13

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('ab
')
Line 3: T_END_HEREDOC ('INNER_END')

Test case 14

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('  ab
')
Line 3: T_END_HEREDOC ('  INNER_END')

Test case 15

Parse error: Invalid body indentation level (expecting an indentation level of at least 3) on line 2

Test case 16

Line 1: T_OPEN_TAG ('<?php ')
Line 1: T_START_HEREDOC ('<<<'INNER_END'
')
Line 2: T_ENCAPSED_AND_WHITESPACE ('
')
Line 3: T_END_HEREDOC ('   INNER_END')

Test case 17

Parse error: syntax error, unexpected end of file, expecting variable or heredoc end or "${" or "{$" on line 4

Test case 18

Parse error: syntax error, unexpected end of file, expecting variable or heredoc end or "${" or "{$" on line 4"#,
        expectf: false,
    },
    Case {
        name: r#"token_get_all_variation10"#,
        source: r#"<?php
/*
 * Using different types of constants in 'source' string to check them for token
 * integer const - T_LNUMBER(305)
 * float/double/real const - T_DNUMBER(306)
 * string cosnt - T_CONSTANT_ESCAPED_STRING(315)
 * bool const (no tokens specified) - T_UNKNOWN(307)
 * null const (no tokens specified) - T_UNKNOWN(307)
*/

echo "*** Testing token_get_all() :  'source' string with different constants ***\n";

$a = 1;
$b = 0;

$source = array (
  // int const
  '<?php $a = 1 + 034; $b = $a + 0x3F; ?>',

  // float const
  '<?php $a = 0.23E-2 + 0.43e2 + 0.5; ?>',

  // string const
  '<?php $a = "hello ".\'world\'; ?>',

  // bool const
  "<?php \$a = (\$b)? true : false; ?>",
  "<?php \$b = (\$a)? FALSE : TRUE; ?>",

  // null const
  '<?php $b = null | NULL; ?>'
);
for($count = 0; $count < count($source); $count++) {
  echo "-- Iteration ".($count + 1)." --\n";
  var_dump( token_get_all($source[$count]));
}

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() :  'source' string with different constants ***
-- Iteration 1 --
array(24) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  string(1) "+"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "034"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [14]=>
  string(1) "="
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [18]=>
  string(1) "+"
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "0x3F"
    [2]=>
    int(1)
  }
  [21]=>
  string(1) ";"
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 2 --
array(17) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "0.23E-2"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  string(1) "+"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "0.43e2"
    [2]=>
    int(1)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [11]=>
  string(1) "+"
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "0.5"
    [2]=>
    int(1)
  }
  [14]=>
  string(1) ";"
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 3 --
array(11) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) ""hello ""
    [2]=>
    int(1)
  }
  [6]=>
  string(1) "."
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "'world'"
    [2]=>
    int(1)
  }
  [8]=>
  string(1) ";"
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 4 --
array(18) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  string(1) "("
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [7]=>
  string(1) ")"
  [8]=>
  string(1) "?"
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "true"
    [2]=>
    int(1)
  }
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  string(1) ":"
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "false"
    [2]=>
    int(1)
  }
  [15]=>
  string(1) ";"
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 5 --
array(18) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  string(1) "("
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [7]=>
  string(1) ")"
  [8]=>
  string(1) "?"
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "FALSE"
    [2]=>
    int(1)
  }
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  string(1) ":"
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "TRUE"
    [2]=>
    int(1)
  }
  [15]=>
  string(1) ";"
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 6 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "null"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  string(1) "|"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "NULL"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation11"#,
        source: r#"<?php
/*
 * Using different control structure keywords
 *   if..else, elseif - T_IF(301), T_ELSEIF(302), T_ELSE(303)
 *   while - T_WHILE(318)
 *   do...while - T_DO(317)
 *   for - T_ENDFOR(320)
 *   foreach - T_ENDFOREACH(322)
 *   switch...case - T_ENDSWITCH(327), T_CASE(329)
 *   break - T_BREAK(331)
 *   continue - T_CONTINUE(332)
*/

echo "*** Testing token_get_all() : for control structure tokens ***\n";

// if..elseif....else
echo "-- with if..elseif..else..tokens --\n";

$source = '<?php
if($a == true) {
     echo "$a = true";
}
elseif($a == false) {
  echo false;
}
else
  echo 1;
?>';

var_dump( token_get_all($source));

// while..., do..while, break, continue
echo "-- with while..., do..while, switch & continue tokens --\n";

$source = "<?php while(true) {
echo 'True';
break;
}
do {
continue;
}while(false); ?>";

var_dump( token_get_all($source));

// for..., foreach( as )
echo "-- with for..foreach( as ) tokens --\n";

$source = '<?php for($count=0; $count < 5; $count++) {
echo $count;
}
foreach($values as $index) {
continue;
} ?>';

var_dump( token_get_all($source));

// switch..case, default
echo "-- with switch...case tokens --\n";

$source = '<?php switch($var)
case 1: break;
default: echo "default"; ?>';

var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : for control structure tokens ***
-- with if..elseif..else..tokens --
array(48) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(2)
  }
  [2]=>
  string(1) "("
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(2)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "=="
    [2]=>
    int(2)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "true"
    [2]=>
    int(2)
  }
  [8]=>
  string(1) ")"
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [10]=>
  string(1) "{"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "
     "
    [2]=>
    int(2)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(%d)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [14]=>
  string(1) """
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(%d)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) " = true"
    [2]=>
    int(%d)
  }
  [17]=>
  string(1) """
  [18]=>
  string(1) ";"
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [20]=>
  string(1) "}"
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "elseif"
    [2]=>
    int(5)
  }
  [23]=>
  string(1) "("
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(5)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "=="
    [2]=>
    int(5)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "false"
    [2]=>
    int(5)
  }
  [29]=>
  string(1) ")"
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [31]=>
  string(1) "{"
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(5)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(6)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "false"
    [2]=>
    int(6)
  }
  [36]=>
  string(1) ";"
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
  [38]=>
  string(1) "}"
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(7)
  }
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "else"
    [2]=>
    int(8)
  }
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(8)
  }
  [42]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(9)
  }
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [44]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(9)
  }
  [45]=>
  string(1) ";"
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(9)
  }
  [47]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(10)
  }
}
-- with while..., do..while, switch & continue tokens --
array(33) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "while"
    [2]=>
    int(1)
  }
  [2]=>
  string(1) "("
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "true"
    [2]=>
    int(1)
  }
  [4]=>
  string(1) ")"
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [6]=>
  string(1) "{"
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(2)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "'True'"
    [2]=>
    int(2)
  }
  [11]=>
  string(1) ";"
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "break"
    [2]=>
    int(%d)
  }
  [14]=>
  string(1) ";"
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [16]=>
  string(1) "}"
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "do"
    [2]=>
    int(5)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [20]=>
  string(1) "{"
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "continue"
    [2]=>
    int(6)
  }
  [23]=>
  string(1) ";"
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
  [25]=>
  string(1) "}"
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "while"
    [2]=>
    int(7)
  }
  [27]=>
  string(1) "("
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "false"
    [2]=>
    int(7)
  }
  [29]=>
  string(1) ")"
  [30]=>
  string(1) ";"
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(7)
  }
}
-- with for..foreach( as ) tokens --
array(45) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "for"
    [2]=>
    int(1)
  }
  [2]=>
  string(1) "("
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "$count"
    [2]=>
    int(1)
  }
  [4]=>
  string(1) "="
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "0"
    [2]=>
    int(1)
  }
  [6]=>
  string(1) ";"
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "$count"
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [10]=>
  string(1) "<"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "5"
    [2]=>
    int(1)
  }
  [13]=>
  string(1) ";"
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "$count"
    [2]=>
    int(1)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "++"
    [2]=>
    int(1)
  }
  [17]=>
  string(1) ")"
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [19]=>
  string(1) "{"
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(1)
  }
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(2)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "$count"
    [2]=>
    int(2)
  }
  [24]=>
  string(1) ";"
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [26]=>
  string(1) "}"
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "foreach"
    [2]=>
    int(4)
  }
  [29]=>
  string(1) "("
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "$values"
    [2]=>
    int(4)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "as"
    [2]=>
    int(4)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "$index"
    [2]=>
    int(4)
  }
  [35]=>
  string(1) ")"
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [37]=>
  string(1) "{"
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "continue"
    [2]=>
    int(5)
  }
  [40]=>
  string(1) ";"
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [42]=>
  string(1) "}"
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [44]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(6)
  }
}
-- with switch...case tokens --
array(23) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "switch"
    [2]=>
    int(1)
  }
  [2]=>
  string(1) "("
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "$var"
    [2]=>
    int(1)
  }
  [4]=>
  string(1) ")"
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "case"
    [2]=>
    int(2)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(2)
  }
  [9]=>
  string(1) ":"
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "break"
    [2]=>
    int(2)
  }
  [12]=>
  string(1) ";"
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "default"
    [2]=>
    int(%d)
  }
  [15]=>
  string(1) ":"
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(%d)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) ""default""
    [2]=>
    int(%d)
  }
  [20]=>
  string(1) ";"
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(%d)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation12"#,
        source: r#"<?php
/*
 * Testing token_get_all() with following predefined language constants:
 *   __FILE__     - T_FILE
 *   __CLASS__    - T_CLASS_C
 *   __TRAIT__    - T_TRAIT_C
 *   __FUNCTION__ - T_FUNC_C
 *   __PROPERTY__ - T_PROPERTY_C
 *   __LINE__     - T_LINE
 *   __METHOD__   - T_METHOD_C
*/

echo "*** Testing token_get_all() : with language constants ***\n";

// parsing __FILE__ token
echo "-- with FILE --\n";
$source = "<?php
\$fp =  fopen(__FILE__, 'r');
?>";
var_dump( token_get_all($source));

// parsing __CLASS__, __TRAIT__ and __FUNCTION__ tokens
echo "-- with CLASS, TRAIT and FUNCTION --\n";
$source = '<?php
class MyClass
{
  echo  __CLASS__;
  echo  __TRAIT__;
  function myFunction()
  {  echo  __FUNCTION__; }
}
?>';
var_dump( token_get_all($source));

// parsing __LINE__ and __METHOD__ tokens
echo "-- with LINE and METHOD --\n";
$source = '<?php
  $a = __LINE__;
  $b = $b.__METHOD__;
?>';
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : with language constants ***
-- with FILE --
array(15) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "$fp"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "  "
    [2]=>
    int(2)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "fopen"
    [2]=>
    int(2)
  }
  [6]=>
  string(1) "("
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "__FILE__"
    [2]=>
    int(2)
  }
  [8]=>
  string(1) ","
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "'r'"
    [2]=>
    int(2)
  }
  [11]=>
  string(1) ")"
  [12]=>
  string(1) ";"
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(%d)
  }
}
-- with CLASS, TRAIT and FUNCTION --
array(35) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "class"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "MyClass"
    [2]=>
    int(2)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [5]=>
  string(1) "{"
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(%d)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(4)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "  "
    [2]=>
    int(4)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) "__CLASS__"
    [2]=>
    int(4)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(%d)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(5)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "  "
    [2]=>
    int(5)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) "__TRAIT__"
    [2]=>
    int(5)
  }
  [15]=>
  string(1) ";"
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(5)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "function"
    [2]=>
    int(6)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "myFunction"
    [2]=>
    int(6)
  }
  [20]=>
  string(1) "("
  [21]=>
  string(1) ")"
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(6)
  }
  [23]=>
  string(1) "{"
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "  "
    [2]=>
    int(7)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(7)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "  "
    [2]=>
    int(7)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(12) "__FUNCTION__"
    [2]=>
    int(7)
  }
  [28]=>
  string(1) ";"
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [30]=>
  string(1) "}"
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(7)
  }
  [32]=>
  string(1) "}"
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(8)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(9)
  }
}
-- with LINE and METHOD --
array(19) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "  "
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(2)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [4]=>
  string(1) "="
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "__LINE__"
    [2]=>
    int(2)
  }
  [7]=>
  string(1) ";"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(2)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(%d)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [11]=>
  string(1) "="
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(%d)
  }
  [14]=>
  string(1) "."
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "__METHOD__"
    [2]=>
    int(%d)
  }
  [16]=>
  string(1) ";"
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(4)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation13"#,
        source: r#"<?php
/*
 * Testing token_get_all() with different class/object keywords
 *   scope related :
 *     static - T_STATIC(346), global - T_GLOBAL(340),
 *     private - T_PRIVATE(343), public - T_PUBLIC(341),
 *     protected - T_PROTECTED(342)
 *   class/object related :
 *     var - T_VAR(347), abstract - T_ABSTRACT(345),
 *     interface - T_INTERFACE(353), class - T_CLASS(352),
 *     extends - T_EXTENDS(354), implements - T_IMPLEMENTS(355), new - T_NEW(299)
*/

echo "*** Testing token_get_all() : with class/object constructs ***\n";

$source = '<?php
interface MyInterface
{
  public const var $var = 10;
}
abstract class MyClass
{
  private var $a;
  public var $b;
  protected var $c;
  static $d;
  final $e = 10;

  abstract public function myFunction($a);
}
class ChildClass extends MyClass implements MyInterface
{
  global $value;
  function myFunction($a)
  {
    $a = new ChildClass();
    if($a instanceof MyClass)
      echo "object created";
  }
}
ChildClass::myFunction(10);
?>';
$tokens =  token_get_all($source);
var_dump($tokens);

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : with class/object constructs ***
array(145) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) "interface"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(11) "MyInterface"
    [2]=>
    int(2)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [5]=>
  string(1) "{"
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(%d)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "public"
    [2]=>
    int(4)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "const"
    [2]=>
    int(4)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "var"
    [2]=>
    int(4)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "$var"
    [2]=>
    int(4)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [15]=>
  string(1) "="
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "10"
    [2]=>
    int(4)
  }
  [18]=>
  string(1) ";"
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [20]=>
  string(1) "}"
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "abstract"
    [2]=>
    int(6)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "class"
    [2]=>
    int(6)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "MyClass"
    [2]=>
    int(6)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
  [28]=>
  string(1) "{"
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(7)
  }
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "private"
    [2]=>
    int(8)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "var"
    [2]=>
    int(8)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(8)
  }
  [35]=>
  string(1) ";"
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(8)
  }
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "public"
    [2]=>
    int(9)
  }
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "var"
    [2]=>
    int(9)
  }
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(9)
  }
  [42]=>
  string(1) ";"
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(9)
  }
  [44]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) "protected"
    [2]=>
    int(10)
  }
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "var"
    [2]=>
    int(10)
  }
  [47]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [48]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(10)
  }
  [49]=>
  string(1) ";"
  [50]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(10)
  }
  [51]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "static"
    [2]=>
    int(11)
  }
  [52]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(11)
  }
  [53]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$d"
    [2]=>
    int(11)
  }
  [54]=>
  string(1) ";"
  [55]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(11)
  }
  [56]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "final"
    [2]=>
    int(12)
  }
  [57]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(12)
  }
  [58]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$e"
    [2]=>
    int(12)
  }
  [59]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(12)
  }
  [60]=>
  string(1) "="
  [61]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(12)
  }
  [62]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "10"
    [2]=>
    int(12)
  }
  [63]=>
  string(1) ";"
  [64]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "

  "
    [2]=>
    int(12)
  }
  [65]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "abstract"
    [2]=>
    int(14)
  }
  [66]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(14)
  }
  [67]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "public"
    [2]=>
    int(14)
  }
  [68]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(14)
  }
  [69]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "function"
    [2]=>
    int(14)
  }
  [70]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(14)
  }
  [71]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "myFunction"
    [2]=>
    int(14)
  }
  [72]=>
  string(1) "("
  [73]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(14)
  }
  [74]=>
  string(1) ")"
  [75]=>
  string(1) ";"
  [76]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(14)
  }
  [77]=>
  string(1) "}"
  [78]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(15)
  }
  [79]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "class"
    [2]=>
    int(16)
  }
  [80]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(16)
  }
  [81]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "ChildClass"
    [2]=>
    int(16)
  }
  [82]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(16)
  }
  [83]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "extends"
    [2]=>
    int(16)
  }
  [84]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(16)
  }
  [85]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "MyClass"
    [2]=>
    int(16)
  }
  [86]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(16)
  }
  [87]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "implements"
    [2]=>
    int(16)
  }
  [88]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(16)
  }
  [89]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(11) "MyInterface"
    [2]=>
    int(16)
  }
  [90]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(16)
  }
  [91]=>
  string(1) "{"
  [92]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(17)
  }
  [93]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "global"
    [2]=>
    int(18)
  }
  [94]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(18)
  }
  [95]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "$value"
    [2]=>
    int(18)
  }
  [96]=>
  string(1) ";"
  [97]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(18)
  }
  [98]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "function"
    [2]=>
    int(19)
  }
  [99]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(19)
  }
  [100]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "myFunction"
    [2]=>
    int(19)
  }
  [101]=>
  string(1) "("
  [102]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(19)
  }
  [103]=>
  string(1) ")"
  [104]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(19)
  }
  [105]=>
  string(1) "{"
  [106]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(20)
  }
  [107]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(21)
  }
  [108]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(21)
  }
  [109]=>
  string(1) "="
  [110]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(21)
  }
  [111]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "new"
    [2]=>
    int(21)
  }
  [112]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(21)
  }
  [113]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "ChildClass"
    [2]=>
    int(21)
  }
  [114]=>
  string(1) "("
  [115]=>
  string(1) ")"
  [116]=>
  string(1) ";"
  [117]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(21)
  }
  [118]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(22)
  }
  [119]=>
  string(1) "("
  [120]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(22)
  }
  [121]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(22)
  }
  [122]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "instanceof"
    [2]=>
    int(22)
  }
  [123]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(22)
  }
  [124]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "MyClass"
    [2]=>
    int(22)
  }
  [125]=>
  string(1) ")"
  [126]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "
      "
    [2]=>
    int(22)
  }
  [127]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(23)
  }
  [128]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(23)
  }
  [129]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(16) ""object created""
    [2]=>
    int(23)
  }
  [130]=>
  string(1) ";"
  [131]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(23)
  }
  [132]=>
  string(1) "}"
  [133]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(24)
  }
  [134]=>
  string(1) "}"
  [135]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(25)
  }
  [136]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "ChildClass"
    [2]=>
    int(26)
  }
  [137]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "::"
    [2]=>
    int(26)
  }
  [138]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "myFunction"
    [2]=>
    int(26)
  }
  [139]=>
  string(1) "("
  [140]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "10"
    [2]=>
    int(26)
  }
  [141]=>
  string(1) ")"
  [142]=>
  string(1) ";"
  [143]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(26)
  }
  [144]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(27)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation14"#,
        source: r#"<?php
/*
 * Testing token_get_all() with 'source' string containing invalid/unknown token value
 *  unknown tokens - T_UNKNOWN(307)
*/

echo "*** Testing token_get_all() : with invalid/unknown tokens ***\n";

// with valid php tags and invalid tokens
echo "-- with valid PHP tags & invlid tokens --\n";
$source = '<?php
struct myStruct {
  variable $a;
  method() { display $a; }
}
?>';
var_dump( token_get_all($source));

// with invalid open tag for testing entire source to be unknown token
echo "-- with invlalid PHP open tag & valid tokens --\n";
$source = '<pli
echo "hello world"; ?>';
var_dump( token_get_all($source));

// with invalid PHP tags and invalid tokens
echo "-- with invalid PHP tags and tokens --\n";
$source = '<PDP display  $a; <';
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : with invalid/unknown tokens ***
-- with valid PHP tags & invlid tokens --
array(28) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "struct"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "myStruct"
    [2]=>
    int(2)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [5]=>
  string(1) "{"
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(2)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "variable"
    [2]=>
    int(%d)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(%d)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(%d)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "method"
    [2]=>
    int(4)
  }
  [13]=>
  string(1) "("
  [14]=>
  string(1) ")"
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [16]=>
  string(1) "{"
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "display"
    [2]=>
    int(4)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(4)
  }
  [21]=>
  string(1) ";"
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [23]=>
  string(1) "}"
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [25]=>
  string(1) "}"
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(6)
  }
}
-- with invlalid PHP open tag & valid tokens --
array(1) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(27) "<pli
echo "hello world"; ?>"
    [2]=>
    int(1)
  }
}
-- with invalid PHP tags and tokens --
array(1) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(19) "<PDP display  $a; <"
    [2]=>
    int(1)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation15"#,
        source: r#"<?php
/*
 * Testing token_get_all() with heredoc 'source' string with all different types of token and heredoc string within
 *     <<<EOT - T_START_HEREDOC(371)
 *     EOT - T_END_HEREDOC(372)
*/

echo "*** Testing token_get_all() : with heredoc source string ***\n";

$source = <<<EOT
<?=
  \$a = 2;
  \$b = 1;
  \$c = <<<EOS
  This is to test
  heredoc string
EOS;
  echo \$a + \$b;
  function myFunction(\$a)
  {
    var_dump(\$a);
  }
  if(\$b < 10) {
    \$b++;
  }
  else
    \$b--;
  while(\$a > 0) {
    echo "*";
    \$a--;
  }
  myFunction(10);
?>
EOT;
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : with heredoc source string ***
array(103) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "<?="
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(2)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [4]=>
  string(1) "="
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(2)
  }
  [7]=>
  string(1) ";"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(2)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(%d)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [11]=>
  string(1) "="
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(%d)
  }
  [14]=>
  string(1) ";"
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(%d)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(4)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [18]=>
  string(1) "="
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "<<<EOS
"
    [2]=>
    int(4)
  }
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(35) "  This is to test
  heredoc string
"
    [2]=>
    int(5)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "EOS"
    [2]=>
    int(7)
  }
  [23]=>
  string(1) ";"
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(7)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(8)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(8)
  }
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [29]=>
  string(1) "+"
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(8)
  }
  [32]=>
  string(1) ";"
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(8)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "function"
    [2]=>
    int(9)
  }
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "myFunction"
    [2]=>
    int(9)
  }
  [37]=>
  string(1) "("
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(9)
  }
  [39]=>
  string(1) ")"
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(9)
  }
  [41]=>
  string(1) "{"
  [42]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(10)
  }
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "var_dump"
    [2]=>
    int(11)
  }
  [44]=>
  string(1) "("
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(11)
  }
  [46]=>
  string(1) ")"
  [47]=>
  string(1) ";"
  [48]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(11)
  }
  [49]=>
  string(1) "}"
  [50]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(12)
  }
  [51]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(13)
  }
  [52]=>
  string(1) "("
  [53]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(13)
  }
  [54]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(13)
  }
  [55]=>
  string(1) "<"
  [56]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(13)
  }
  [57]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "10"
    [2]=>
    int(13)
  }
  [58]=>
  string(1) ")"
  [59]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(13)
  }
  [60]=>
  string(1) "{"
  [61]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(13)
  }
  [62]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(14)
  }
  [63]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "++"
    [2]=>
    int(14)
  }
  [64]=>
  string(1) ";"
  [65]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(14)
  }
  [66]=>
  string(1) "}"
  [67]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(15)
  }
  [68]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "else"
    [2]=>
    int(16)
  }
  [69]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(16)
  }
  [70]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(17)
  }
  [71]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "--"
    [2]=>
    int(17)
  }
  [72]=>
  string(1) ";"
  [73]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(17)
  }
  [74]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "while"
    [2]=>
    int(18)
  }
  [75]=>
  string(1) "("
  [76]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(18)
  }
  [77]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(18)
  }
  [78]=>
  string(1) ">"
  [79]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(18)
  }
  [80]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "0"
    [2]=>
    int(18)
  }
  [81]=>
  string(1) ")"
  [82]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(18)
  }
  [83]=>
  string(1) "{"
  [84]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(18)
  }
  [85]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(19)
  }
  [86]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(19)
  }
  [87]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) ""*""
    [2]=>
    int(19)
  }
  [88]=>
  string(1) ";"
  [89]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(19)
  }
  [90]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(20)
  }
  [91]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "--"
    [2]=>
    int(20)
  }
  [92]=>
  string(1) ";"
  [93]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(20)
  }
  [94]=>
  string(1) "}"
  [95]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(21)
  }
  [96]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "myFunction"
    [2]=>
    int(22)
  }
  [97]=>
  string(1) "("
  [98]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "10"
    [2]=>
    int(22)
  }
  [99]=>
  string(1) ")"
  [100]=>
  string(1) ";"
  [101]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(22)
  }
  [102]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(23)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation16"#,
        source: r#"<?php
/*
 * Testing token_get_all() with different function keywords
 *   function - T_FUNCTION(333), return - T_RETURN(335)
 *   different functions:
 *     include() - T_INCLUDE(262), print() - T_PRINT(266),
 *     isset() - T_ISSET(349), list() - T_LIST(358),
 *     require() - T_REQUIRE(259), empty() - T_EMPTY(350),
 *     declare() - T_DECLARE(324), array() - T_ARRAY(359),
 *      __halt_compiler() - T_HALT_COMPILER(351)
*/

echo "*** Testing token_get_all() : with different function constructs ***\n";

$source = '<?php
declare(VALUE=100);
include("addfile.php");
require("sumfile.php");

function myFunction($a)
{
  if($a % 2)
    return 1;
  else
    exit;
}

$a = VALUE;
$b = 20;
$c = array(1,2);
$b >>= 2;

if($b <= 0)
  die;
else
  print($b);

list($value1,$value2) = $c;
if(empty($value1) && !isset($value1)) {
  myFunction();
}
?>';
$tokens =  token_get_all($source);
var_dump($tokens);

echo "Done";;
"#,
        expected: r#"*** Testing token_get_all() : with different function constructs ***
array(142) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "declare"
    [2]=>
    int(2)
  }
  [2]=>
  string(1) "("
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "VALUE"
    [2]=>
    int(2)
  }
  [4]=>
  string(1) "="
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "100"
    [2]=>
    int(2)
  }
  [6]=>
  string(1) ")"
  [7]=>
  string(1) ";"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "include"
    [2]=>
    int(3)
  }
  [10]=>
  string(1) "("
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(13) ""addfile.php""
    [2]=>
    int(3)
  }
  [12]=>
  string(1) ")"
  [13]=>
  string(1) ";"
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(3)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "require"
    [2]=>
    int(4)
  }
  [16]=>
  string(1) "("
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(13) ""sumfile.php""
    [2]=>
    int(4)
  }
  [18]=>
  string(1) ")"
  [19]=>
  string(1) ";"
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "

"
    [2]=>
    int(4)
  }
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "function"
    [2]=>
    int(6)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "myFunction"
    [2]=>
    int(6)
  }
  [24]=>
  string(1) "("
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(6)
  }
  [26]=>
  string(1) ")"
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
  [28]=>
  string(1) "{"
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(7)
  }
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(8)
  }
  [31]=>
  string(1) "("
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(8)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [34]=>
  string(1) "%"
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(8)
  }
  [37]=>
  string(1) ")"
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(8)
  }
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "return"
    [2]=>
    int(9)
  }
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(9)
  }
  [42]=>
  string(1) ";"
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(9)
  }
  [44]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "else"
    [2]=>
    int(10)
  }
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(10)
  }
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "exit"
    [2]=>
    int(11)
  }
  [47]=>
  string(1) ";"
  [48]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(11)
  }
  [49]=>
  string(1) "}"
  [50]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "

"
    [2]=>
    int(12)
  }
  [51]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(14)
  }
  [52]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(14)
  }
  [53]=>
  string(1) "="
  [54]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(14)
  }
  [55]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "VALUE"
    [2]=>
    int(14)
  }
  [56]=>
  string(1) ";"
  [57]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(14)
  }
  [58]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(15)
  }
  [59]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(15)
  }
  [60]=>
  string(1) "="
  [61]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(15)
  }
  [62]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "20"
    [2]=>
    int(15)
  }
  [63]=>
  string(1) ";"
  [64]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(15)
  }
  [65]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(16)
  }
  [66]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(16)
  }
  [67]=>
  string(1) "="
  [68]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(16)
  }
  [69]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "array"
    [2]=>
    int(16)
  }
  [70]=>
  string(1) "("
  [71]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(16)
  }
  [72]=>
  string(1) ","
  [73]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(16)
  }
  [74]=>
  string(1) ")"
  [75]=>
  string(1) ";"
  [76]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(16)
  }
  [77]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(17)
  }
  [78]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(17)
  }
  [79]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) ">>="
    [2]=>
    int(17)
  }
  [80]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(17)
  }
  [81]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(17)
  }
  [82]=>
  string(1) ";"
  [83]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "

"
    [2]=>
    int(17)
  }
  [84]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(19)
  }
  [85]=>
  string(1) "("
  [86]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(19)
  }
  [87]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(19)
  }
  [88]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "<="
    [2]=>
    int(19)
  }
  [89]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(19)
  }
  [90]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "0"
    [2]=>
    int(19)
  }
  [91]=>
  string(1) ")"
  [92]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(19)
  }
  [93]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "die"
    [2]=>
    int(20)
  }
  [94]=>
  string(1) ";"
  [95]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(20)
  }
  [96]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "else"
    [2]=>
    int(21)
  }
  [97]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(21)
  }
  [98]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "print"
    [2]=>
    int(22)
  }
  [99]=>
  string(1) "("
  [100]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(22)
  }
  [101]=>
  string(1) ")"
  [102]=>
  string(1) ";"
  [103]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "

"
    [2]=>
    int(22)
  }
  [104]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "list"
    [2]=>
    int(24)
  }
  [105]=>
  string(1) "("
  [106]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "$value1"
    [2]=>
    int(24)
  }
  [107]=>
  string(1) ","
  [108]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "$value2"
    [2]=>
    int(24)
  }
  [109]=>
  string(1) ")"
  [110]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(24)
  }
  [111]=>
  string(1) "="
  [112]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(24)
  }
  [113]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(24)
  }
  [114]=>
  string(1) ";"
  [115]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(24)
  }
  [116]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(25)
  }
  [117]=>
  string(1) "("
  [118]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "empty"
    [2]=>
    int(25)
  }
  [119]=>
  string(1) "("
  [120]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "$value1"
    [2]=>
    int(25)
  }
  [121]=>
  string(1) ")"
  [122]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(25)
  }
  [123]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "&&"
    [2]=>
    int(25)
  }
  [124]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(25)
  }
  [125]=>
  string(1) "!"
  [126]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "isset"
    [2]=>
    int(25)
  }
  [127]=>
  string(1) "("
  [128]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "$value1"
    [2]=>
    int(25)
  }
  [129]=>
  string(1) ")"
  [130]=>
  string(1) ")"
  [131]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(25)
  }
  [132]=>
  string(1) "{"
  [133]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(25)
  }
  [134]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(10) "myFunction"
    [2]=>
    int(26)
  }
  [135]=>
  string(1) "("
  [136]=>
  string(1) ")"
  [137]=>
  string(1) ";"
  [138]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(26)
  }
  [139]=>
  string(1) "}"
  [140]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(27)
  }
  [141]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(28)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation17"#,
        source: r#"<?php
/*
 * Testing token_get_all() with different exception keywords
 *   try - T_TRY(336),
 *   catch - T_CATCH(337),
 *   throw - T_THROW(338)
*/

echo "*** Testing token_get_all() : with exception keywords ***\n";

$source = '<?php
function inverse($x)
{
  if($x == 0) {
    throw new Exception("Division by zero");
  else
    return 1/$x;
}
try {
  echo inverse(0);
  echo inverse(5);
} catch(Exception $e) {
    echo "caught exception:";
}
}
?>';
$tokens =  token_get_all($source);
var_dump($tokens);

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : with exception keywords ***
array(83) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "function"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "inverse"
    [2]=>
    int(2)
  }
  [4]=>
  string(1) "("
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$x"
    [2]=>
    int(2)
  }
  [6]=>
  string(1) ")"
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [8]=>
  string(1) "{"
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(%d)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(4)
  }
  [11]=>
  string(1) "("
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$x"
    [2]=>
    int(4)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "=="
    [2]=>
    int(4)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "0"
    [2]=>
    int(4)
  }
  [17]=>
  string(1) ")"
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [19]=>
  string(1) "{"
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(4)
  }
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "throw"
    [2]=>
    int(5)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "new"
    [2]=>
    int(5)
  }
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) "Exception"
    [2]=>
    int(5)
  }
  [26]=>
  string(1) "("
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(18) ""Division by zero""
    [2]=>
    int(5)
  }
  [28]=>
  string(1) ")"
  [29]=>
  string(1) ";"
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(5)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "else"
    [2]=>
    int(6)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(6)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "return"
    [2]=>
    int(7)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(7)
  }
  [36]=>
  string(1) "/"
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$x"
    [2]=>
    int(7)
  }
  [38]=>
  string(1) ";"
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(7)
  }
  [40]=>
  string(1) "}"
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(8)
  }
  [42]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "try"
    [2]=>
    int(9)
  }
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [44]=>
  string(1) "{"
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(9)
  }
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(10)
  }
  [47]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [48]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "inverse"
    [2]=>
    int(10)
  }
  [49]=>
  string(1) "("
  [50]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "0"
    [2]=>
    int(10)
  }
  [51]=>
  string(1) ")"
  [52]=>
  string(1) ";"
  [53]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(10)
  }
  [54]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(11)
  }
  [55]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(11)
  }
  [56]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "inverse"
    [2]=>
    int(11)
  }
  [57]=>
  string(1) "("
  [58]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "5"
    [2]=>
    int(11)
  }
  [59]=>
  string(1) ")"
  [60]=>
  string(1) ";"
  [61]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(11)
  }
  [62]=>
  string(1) "}"
  [63]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(12)
  }
  [64]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "catch"
    [2]=>
    int(12)
  }
  [65]=>
  string(1) "("
  [66]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) "Exception"
    [2]=>
    int(12)
  }
  [67]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(12)
  }
  [68]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$e"
    [2]=>
    int(12)
  }
  [69]=>
  string(1) ")"
  [70]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(12)
  }
  [71]=>
  string(1) "{"
  [72]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "
    "
    [2]=>
    int(12)
  }
  [73]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(13)
  }
  [74]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(13)
  }
  [75]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(19) ""caught exception:""
    [2]=>
    int(13)
  }
  [76]=>
  string(1) ";"
  [77]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(13)
  }
  [78]=>
  string(1) "}"
  [79]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(14)
  }
  [80]=>
  string(1) "}"
  [81]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(15)
  }
  [82]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(16)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation18"#,
        source: r#"<?php
/*
 * Testing token_get_all() with source string containing HTML code with PHP
 *   HTML tags are considered to be T_INLINE_HTML(311)
*/

echo "*** Testing token_get_all() : 'source' string with HTML tags ***\n";

$source = '
<html>
<body>
    Testing HTML
</body>
</html>"

<?php
    echo "php code with HTML";
?>';
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : 'source' string with HTML tags ***
array(9) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(50) "
<html>
<body>
    Testing HTML
</body>
</html>"

"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(8)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "    "
    [2]=>
    int(9)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(9)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(20) ""php code with HTML""
    [2]=>
    int(9)
  }
  [6]=>
  string(1) ";"
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(9)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(10)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation19"#,
        source: r#"<?php

$phpstr = '
<?php

// A php script to test token_get_all()

/* a different
type of
comment */

// a class
class TestClass {
    public function foo() {
        echo "Called foo()\n";
    }
}

$a = new TestClass();
$a->foo();

for ($i = 0; $i < 10; $i++) {
    echo "Loop iteration $i\n";
}

?>';

$token_array = token_get_all($phpstr);

$script = "";
// reconstruct a script (without open/close tags) from the token array
foreach ($token_array as $token) {
    if (is_array($token)) {
        if (strncmp($token[1], '<?php', 5) == 0) {
            continue;
        }
        if (strncmp($token[1], '?>', 2) == 0) {
            continue;
        }
        $script .= $token[1];
    } else {
        $script .= $token;
    }
}

var_dump($script);

eval($script);;
"#,
        expected: r#"string(274) "

// A php script to test token_get_all()

/* a different
type of
comment */

// a class
class TestClass {
    public function foo() {
        echo "Called foo()\n";
    }
}

$a = new TestClass();
$a->foo();

for ($i = 0; $i < 10; $i++) {
    echo "Loop iteration $i\n";
}

"
Called foo()
Loop iteration 0
Loop iteration 1
Loop iteration 2
Loop iteration 3
Loop iteration 4
Loop iteration 5
Loop iteration 6
Loop iteration 7
Loop iteration 8
Loop iteration 9"#,
        expectf: false,
    },
    Case {
        name: r#"token_get_all_variation2"#,
        source: r#"<?php
/*
 * Passing 'source' argument with different arithmetic operators to test them for token
 * Arithmetic operators: +, -, *, /, % are not listed as specific operator tokens,
 *    so they are expected to return string - T_STRING
*/

echo "*** Testing token_get_all() : 'source' string with different arithmetic operators ***\n";

// arithmetic operators - '+', '-', '*', '/', '%'
$source = array (
  '<?php $a = 1 + 2; ?>',
  '<?php $b = $b - 2; ?>',
  '<?php $c = $a * $b; ?>',
  '<?php $a = $b % 2; ?>'
);
for($count = 0; $count < count($source); $count++) {
  echo "-- Iteration ".($count + 1)." --\n";
  var_dump( token_get_all($source[$count]));
}
echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : 'source' string with different arithmetic operators ***
-- Iteration 1 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  string(1) "+"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 2 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  string(1) "-"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 3 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  string(1) "*"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 4 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  string(1) "%"
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation3"#,
        source: r#"<?php
/*
 * Passing 'source' argument with different logical operators to test them for tokens
 *   and - T_AND_LOGICAL_AND(265),
 *   or - T_LOGICAL_OR(263),
 *   xor - T_LOGICAL_XOR(264),
 *   && - T_BOOLEAN_AND(279),
 *   || - T_BOOLEAN_OR(278)
*/

echo "*** Testing token_get_all() : 'source' string with different logical operators ***\n";

// logical operators : 'and', 'or', 'xor', '&&', '||'
$source = array (
  '<?php $a = 1 and 024; ?>',
  '<?php $b = $b or 0X1E; ?>',
  '<?php $c = $a xor $b; ?>',
  '<?php $a = $b && 2; ?>',
  '<?php $b = $b || 1; ?>'
);
for($count = 0; $count < count($source); $count++) {
  echo "-- Iteration ".($count + 1)." --\n";
  var_dump( token_get_all($source[$count]));
}

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : 'source' string with different logical operators ***
-- Iteration 1 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "and"
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "024"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 2 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "or"
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "0X1E"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 3 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "xor"
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 4 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "&&"
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
-- Iteration 5 --
array(13) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php "
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(1)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "||"
    [2]=>
    int(1)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(1)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(1)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(1)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation4"#,
        source: r#"<?php
/*
 * Passing 'source' argument with different comparison operators to test them for tokens
 *  == - T_IS_EQUAL(283), === - T_IS_IDENTICAL(281),
 *  >= - T_IS_GREATER_OR_EQUAL(284), <= - T_IS_LESS_OR_EQUAL(285)
 *  != - T_IS_NOT_EQUAL, <> - T_IS_NOT_EQUAL(282), !== - T_IS_NOT_IDENTICAL(280)
*/

echo "*** Testing token_get_all() : 'source' string with different comparison operators ***\n";

// comparison operators : '==', '===', '>=', '<=', '!=', '!==', '<>'
$source = '<?php
if($a == 0)
  echo "== 0";
elseif($a === 2)
  echo "=== 2";
elseif($a >= 10 && $a <= 20)
  echo ">= 10 & <=20";
elseif($a != 1 || $a <> 1)
  echo "!= 1";
elseif($a !== 1)
  echo "!==1";
?>';
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : 'source' string with different comparison operators ***
array(88) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(2)
  }
  [2]=>
  string(1) "("
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(2)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "=="
    [2]=>
    int(2)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "0"
    [2]=>
    int(2)
  }
  [8]=>
  string(1) ")"
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(2)
  }
  [10]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(%d)
  }
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) ""== 0""
    [2]=>
    int(%d)
  }
  [13]=>
  string(1) ";"
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "elseif"
    [2]=>
    int(4)
  }
  [16]=>
  string(1) "("
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(4)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "==="
    [2]=>
    int(4)
  }
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(4)
  }
  [22]=>
  string(1) ")"
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(4)
  }
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(5)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) ""=== 2""
    [2]=>
    int(5)
  }
  [27]=>
  string(1) ";"
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "elseif"
    [2]=>
    int(6)
  }
  [30]=>
  string(1) "("
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(6)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) ">="
    [2]=>
    int(6)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "10"
    [2]=>
    int(6)
  }
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "&&"
    [2]=>
    int(6)
  }
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(6)
  }
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "<="
    [2]=>
    int(6)
  }
  [42]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "20"
    [2]=>
    int(6)
  }
  [44]=>
  string(1) ")"
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(6)
  }
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(7)
  }
  [47]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [48]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(14) "">= 10 & <=20""
    [2]=>
    int(7)
  }
  [49]=>
  string(1) ";"
  [50]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(7)
  }
  [51]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "elseif"
    [2]=>
    int(8)
  }
  [52]=>
  string(1) "("
  [53]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(8)
  }
  [54]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [55]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "!="
    [2]=>
    int(8)
  }
  [56]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [57]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(8)
  }
  [58]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [59]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "||"
    [2]=>
    int(8)
  }
  [60]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [61]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(8)
  }
  [62]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [63]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "<>"
    [2]=>
    int(8)
  }
  [64]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [65]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(8)
  }
  [66]=>
  string(1) ")"
  [67]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(8)
  }
  [68]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(9)
  }
  [69]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [70]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) ""!= 1""
    [2]=>
    int(9)
  }
  [71]=>
  string(1) ";"
  [72]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(9)
  }
  [73]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "elseif"
    [2]=>
    int(10)
  }
  [74]=>
  string(1) "("
  [75]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(10)
  }
  [76]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [77]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "!=="
    [2]=>
    int(10)
  }
  [78]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [79]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(10)
  }
  [80]=>
  string(1) ")"
  [81]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "
  "
    [2]=>
    int(10)
  }
  [82]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(11)
  }
  [83]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(11)
  }
  [84]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) ""!==1""
    [2]=>
    int(11)
  }
  [85]=>
  string(1) ";"
  [86]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(11)
  }
  [87]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(12)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation5"#,
        source: r#"<?php
/*
 * Passing 'source' argument with different assignment operators to test them for tokens
 *  += - T_PLUS_EQUAL(277), -= - T_MINUS-EQUAL(276),
 *  *= - T_MUL_EQUAL(275), /= - T_DIVIDE_EQUAL(274),
 *  %= - T_MOD_EQUAL(272), &= - T_AND_EQUAL(271),
 *  |= - T_OR_EQUAL(271), ^= - T_EXOR_EQUAL(269),
 *  >>= - T_SR_EQUAL(267), <<= - T_SL_EQUAL(268), .= - T_CONCAT_EQUAL(273)
*/

echo "*** Testing token_get_all() : 'source' string with different assignment operators ***\n";

// assignment operators : '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=', '>>=', '<<=', '.='
$source = '<?php
$a = 1, $b = 2;
$c += $b;
$b -= $a;
$a *= 2;
$d /= 10.50;
$a %= 10.50;
$b &= $c;
$c |= 1;
$d ^= 5;
$a >>= 1;
$b <<= 2;
$d .= "hello world";
?>';
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : 'source' string with different assignment operators ***
array(93) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(2)
  }
  [6]=>
  string(1) ","
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(2)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [10]=>
  string(1) "="
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(2)
  }
  [13]=>
  string(1) ";"
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(%d)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "+="
    [2]=>
    int(%d)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(%d)
  }
  [20]=>
  string(1) ";"
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(4)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "-="
    [2]=>
    int(4)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(4)
  }
  [27]=>
  string(1) ";"
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(5)
  }
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "*="
    [2]=>
    int(5)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(5)
  }
  [34]=>
  string(1) ";"
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$d"
    [2]=>
    int(6)
  }
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "/="
    [2]=>
    int(6)
  }
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "10.50"
    [2]=>
    int(6)
  }
  [41]=>
  string(1) ";"
  [42]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(7)
  }
  [44]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "%="
    [2]=>
    int(7)
  }
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [47]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "10.50"
    [2]=>
    int(7)
  }
  [48]=>
  string(1) ";"
  [49]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(7)
  }
  [50]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(8)
  }
  [51]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [52]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "&="
    [2]=>
    int(8)
  }
  [53]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [54]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(8)
  }
  [55]=>
  string(1) ";"
  [56]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(8)
  }
  [57]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(9)
  }
  [58]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [59]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "|="
    [2]=>
    int(9)
  }
  [60]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [61]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(9)
  }
  [62]=>
  string(1) ";"
  [63]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(9)
  }
  [64]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$d"
    [2]=>
    int(10)
  }
  [65]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [66]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "^="
    [2]=>
    int(10)
  }
  [67]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [68]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "5"
    [2]=>
    int(10)
  }
  [69]=>
  string(1) ";"
  [70]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(10)
  }
  [71]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(11)
  }
  [72]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(11)
  }
  [73]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) ">>="
    [2]=>
    int(11)
  }
  [74]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(11)
  }
  [75]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(11)
  }
  [76]=>
  string(1) ";"
  [77]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(11)
  }
  [78]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(12)
  }
  [79]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(12)
  }
  [80]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(3) "<<="
    [2]=>
    int(12)
  }
  [81]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(12)
  }
  [82]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(12)
  }
  [83]=>
  string(1) ";"
  [84]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(12)
  }
  [85]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$d"
    [2]=>
    int(13)
  }
  [86]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(13)
  }
  [87]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) ".="
    [2]=>
    int(13)
  }
  [88]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(13)
  }
  [89]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(13) ""hello world""
    [2]=>
    int(13)
  }
  [90]=>
  string(1) ";"
  [91]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(13)
  }
  [92]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(14)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation6"#,
        source: r#"<?php
/*
 * Passing 'source' argument with different bitwise operators to test them for token
 *  << - T_SL(287)
 *  >> - T_SR(286)
*/

echo "*** Testing token_get_all() : 'source' string with different bitwise operators ***\n";

// bitwise operators : '<<' , '>>'
$source = '<?php
$a = 2, $b = 4;
$a = $a << 2;
$b = $b >> 2;
var_dump($a);
var_dump($b);
?>';
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : 'source' string with different bitwise operators ***
array(50) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(2)
  }
  [6]=>
  string(1) ","
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(2)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [10]=>
  string(1) "="
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "4"
    [2]=>
    int(2)
  }
  [13]=>
  string(1) ";"
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(%d)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [17]=>
  string(1) "="
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(%d)
  }
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [21]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "<<"
    [2]=>
    int(%d)
  }
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(%d)
  }
  [24]=>
  string(1) ";"
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(4)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [28]=>
  string(1) "="
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(4)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) ">>"
    [2]=>
    int(4)
  }
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "2"
    [2]=>
    int(4)
  }
  [35]=>
  string(1) ";"
  [36]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "var_dump"
    [2]=>
    int(5)
  }
  [38]=>
  string(1) "("
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(5)
  }
  [40]=>
  string(1) ")"
  [41]=>
  string(1) ";"
  [42]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "var_dump"
    [2]=>
    int(6)
  }
  [44]=>
  string(1) "("
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(6)
  }
  [46]=>
  string(1) ")"
  [47]=>
  string(1) ";"
  [48]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
  [49]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(7)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation7"#,
        source: r#"<?php
/*
 * Passing 'source' argument with different increment/decrement operators to test them for token
 *  ++ - T_INC(297)
 *  -- - T_DEC(296)
*/

echo "*** Testing token_get_all() : 'source' string with different increment/decrement operators ***\n";

// increment/decrement operators : '++' , '--'
$source = '<?php
$a = 10, $b = 5;
$a++;
$b--;
echo $a;
?>';
var_dump(token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : 'source' string with different increment/decrement operators ***
array(29) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "10"
    [2]=>
    int(2)
  }
  [6]=>
  string(1) ","
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(2)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [10]=>
  string(1) "="
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "5"
    [2]=>
    int(2)
  }
  [13]=>
  string(1) ";"
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(%d)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "++"
    [2]=>
    int(%d)
  }
  [17]=>
  string(1) ";"
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(4)
  }
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "--"
    [2]=>
    int(4)
  }
  [21]=>
  string(1) ";"
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(5)
  }
  [24]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(5)
  }
  [26]=>
  string(1) ";"
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(6)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation8"#,
        source: r#"<?php
/*
 * Passing 'source' argument with different type casting operators to test them for token
 *  (int)/(integer) - T_INT_CAST(295), (float)/(real)/(double) - T_DOUBLE_CAST(294),
 *  (string) - T_STRING_CAST(293), (bool)/(boolean) - T_BOOL_CAST(290),
 *  (unset) - T_UNSET_CAST(289)
*/

echo "*** Testing token_get_all() : 'source' string with different type casting operators ***\n";

// type casting operators : (int), (integer), (float), (double), (string), (array), (object), (bool), (boolean),(unset)
$source = '<?php
$a = 1, $b = 10.5
$c = (int)$b + $a;
$d = (float)$a + $b;
$e = (string)$a.(string)$b;
if((bool)$a) echo "true";
if(!(boolean)$b) echo "false";
$c = $c + (integer) 123.4e2;
$d = $c - (real) 12;
$b = (unset)$a;
?>';
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r#"*** Testing token_get_all() : 'source' string with different type casting operators ***
array(107) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [3]=>
  string(1) "="
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "1"
    [2]=>
    int(2)
  }
  [6]=>
  string(1) ","
  [7]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(2)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [10]=>
  string(1) "="
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(2)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "10.5"
    [2]=>
    int(2)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(2)
  }
  [14]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(%d)
  }
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [16]=>
  string(1) "="
  [17]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(5) "(int)"
    [2]=>
    int(%d)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(%d)
  }
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [21]=>
  string(1) "+"
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(%d)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(%d)
  }
  [24]=>
  string(1) ";"
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(%d)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$d"
    [2]=>
    int(4)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [28]=>
  string(1) "="
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "(float)"
    [2]=>
    int(4)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(4)
  }
  [32]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [33]=>
  string(1) "+"
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(4)
  }
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(4)
  }
  [36]=>
  string(1) ";"
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(4)
  }
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$e"
    [2]=>
    int(5)
  }
  [39]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [40]=>
  string(1) "="
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(5)
  }
  [42]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "(string)"
    [2]=>
    int(5)
  }
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(5)
  }
  [44]=>
  string(1) "."
  [45]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "(string)"
    [2]=>
    int(5)
  }
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(5)
  }
  [47]=>
  string(1) ";"
  [48]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(5)
  }
  [49]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(6)
  }
  [50]=>
  string(1) "("
  [51]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "(bool)"
    [2]=>
    int(6)
  }
  [52]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(6)
  }
  [53]=>
  string(1) ")"
  [54]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [55]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(6)
  }
  [56]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(6)
  }
  [57]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) ""true""
    [2]=>
    int(6)
  }
  [58]=>
  string(1) ";"
  [59]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
  [60]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "if"
    [2]=>
    int(7)
  }
  [61]=>
  string(1) "("
  [62]=>
  string(1) "!"
  [63]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) "(boolean)"
    [2]=>
    int(7)
  }
  [64]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(7)
  }
  [65]=>
  string(1) ")"
  [66]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [67]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "echo"
    [2]=>
    int(7)
  }
  [68]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [69]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) ""false""
    [2]=>
    int(7)
  }
  [70]=>
  string(1) ";"
  [71]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(7)
  }
  [72]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(8)
  }
  [73]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [74]=>
  string(1) "="
  [75]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [76]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(8)
  }
  [77]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [78]=>
  string(1) "+"
  [79]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [80]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(9) "(integer)"
    [2]=>
    int(8)
  }
  [81]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [82]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "123.4e2"
    [2]=>
    int(8)
  }
  [83]=>
  string(1) ";"
  [84]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(8)
  }
  [85]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$d"
    [2]=>
    int(9)
  }
  [86]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [87]=>
  string(1) "="
  [88]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [89]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(9)
  }
  [90]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [91]=>
  string(1) "-"
  [92]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [93]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "(real)"
    [2]=>
    int(9)
  }
  [94]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [95]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "12"
    [2]=>
    int(9)
  }
  [96]=>
  string(1) ";"
  [97]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(9)
  }
  [98]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(10)
  }
  [99]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [100]=>
  string(1) "="
  [101]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(10)
  }
  [102]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(7) "(unset)"
    [2]=>
    int(10)
  }
  [103]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(10)
  }
  [104]=>
  string(1) ";"
  [105]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(10)
  }
  [106]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(11)
  }
}
Done"#,
        expectf: true,
    },
    Case {
        name: r#"token_get_all_variation9"#,
        source: r#"<?php
/*
 * Passing 'source' argument with different style of comments
 */
//  '//', '/* */', '#' - T_COMMENT(365)
// '/** */' - T_DOC_COMMENT(366)


echo "*** Testing token_get_all() : 'source' string with different comments ***\n";

// types of comments: '//', '/* */', '#' & /** */

$source = '<?php
/** Performing addition operation on given values :
  * a, b
  */

// int value
$a = 10;
$b = 20;
$c = true; // bool value

/*
 * Performing operation on $a,$b
 * display result
 */
$c = $a + $b;
var_dump($c); # expected: int(30)

# end of program
?>';
var_dump( token_get_all($source));

echo "Done";
"#,
        expected: r##"*** Testing token_get_all() : 'source' string with different comments ***
array(52) {
  [0]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(6) "<?php
"
    [2]=>
    int(1)
  }
  [1]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(65) "/** Performing addition operation on given values :
  * a, b
  */"
    [2]=>
    int(2)
  }
  [2]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "

"
    [2]=>
    int(4)
  }
  [3]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(12) "// int value"
    [2]=>
    int(6)
  }
  [4]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(6)
  }
  [5]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(7)
  }
  [6]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [7]=>
  string(1) "="
  [8]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(7)
  }
  [9]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "10"
    [2]=>
    int(7)
  }
  [10]=>
  string(1) ";"
  [11]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(7)
  }
  [12]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(8)
  }
  [13]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [14]=>
  string(1) "="
  [15]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(8)
  }
  [16]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "20"
    [2]=>
    int(8)
  }
  [17]=>
  string(1) ";"
  [18]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(8)
  }
  [19]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(9)
  }
  [20]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [21]=>
  string(1) "="
  [22]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [23]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(4) "true"
    [2]=>
    int(9)
  }
  [24]=>
  string(1) ";"
  [25]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(9)
  }
  [26]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(13) "// bool value"
    [2]=>
    int(9)
  }
  [27]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "

"
    [2]=>
    int(9)
  }
  [28]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(57) "/*
 * Performing operation on $a,$b
 * display result
 */"
    [2]=>
    int(11)
  }
  [29]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(14)
  }
  [30]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(15)
  }
  [31]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(15)
  }
  [32]=>
  string(1) "="
  [33]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(15)
  }
  [34]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$a"
    [2]=>
    int(15)
  }
  [35]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(15)
  }
  [36]=>
  string(1) "+"
  [37]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(15)
  }
  [38]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$b"
    [2]=>
    int(15)
  }
  [39]=>
  string(1) ";"
  [40]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(15)
  }
  [41]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(8) "var_dump"
    [2]=>
    int(16)
  }
  [42]=>
  string(1) "("
  [43]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "$c"
    [2]=>
    int(16)
  }
  [44]=>
  string(1) ")"
  [45]=>
  string(1) ";"
  [46]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) " "
    [2]=>
    int(16)
  }
  [47]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(19) "# expected: int(30)"
    [2]=>
    int(16)
  }
  [48]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "

"
    [2]=>
    int(16)
  }
  [49]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(16) "# end of program"
    [2]=>
    int(18)
  }
  [50]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(1) "
"
    [2]=>
    int(18)
  }
  [51]=>
  array(3) {
    [0]=>
    int(%d)
    [1]=>
    string(2) "?>"
    [2]=>
    int(19)
  }
}
Done"##,
        expectf: true,
    },
];
