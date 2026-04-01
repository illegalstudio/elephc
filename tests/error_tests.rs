use elephc::lexer::tokenize;
use elephc::parser::parse;
use elephc::types;
use elephc::types::PhpType;
use std::collections::HashSet;

fn check_source(src: &str) -> Result<(), String> {
    check_source_with_defines(src, &[])
}

fn check_source_with_defines(src: &str, defines: &[&str]) -> Result<(), String> {
    let tokens = tokenize(src).map_err(|e| e.message.clone())?;
    let ast = parse(&tokens).map_err(|e| e.message.clone())?;
    let define_set: HashSet<String> = defines.iter().map(|define| (*define).to_string()).collect();
    let ast = elephc::conditional::apply(ast, &define_set);
    let ast = elephc::name_resolver::resolve(ast).map_err(|e| e.message.clone())?;
    types::check(&ast).map_err(|e| e.message.clone())?;
    Ok(())
}

fn expect_error(src: &str, expected_substr: &str) {
    match check_source(src) {
        Ok(_) => panic!(
            "Expected error containing '{}', but got Ok",
            expected_substr
        ),
        Err(msg) => {
            assert!(
                msg.contains(expected_substr),
                "Error '{}' doesn't contain '{}'",
                msg,
                expected_substr,
            );
        }
    }
}

macro_rules! expect_builtin_arity_error {
    ($test_name:ident, $src:expr, $expected:expr) => {
        #[test]
        fn $test_name() {
            expect_error($src, $expected);
        }
    };
}

// --- Lexer errors ---

#[test]
fn test_error_missing_open_tag() {
    expect_error("echo \"hi\";", "<?php");
}

#[test]
fn test_error_unterminated_string() {
    expect_error("<?php \"no end", "Unterminated string");
}

#[test]
fn test_error_empty_variable() {
    expect_error("<?php $;", "Expected variable name");
}

#[test]
fn test_error_bare_identifier() {
    expect_error("<?php foo;", "Undefined constant: foo");
}

#[test]
fn test_error_unexpected_character() {
    expect_error("<?php @", "Unexpected character");
}

#[test]
fn test_error_single_ampersand() {
    expect_error("<?php &;", "Unexpected token");
}

#[test]
fn test_error_single_pipe() {
    expect_error("<?php |;", "Unexpected token");
}

// --- Parser errors ---

#[test]
fn test_error_missing_semicolon() {
    expect_error("<?php echo \"hi\"", "Expected ';'");
}

#[test]
fn test_error_missing_equals() {
    expect_error("<?php $x \"hi\";", "Expected '='");
}

#[test]
fn test_error_unclosed_paren() {
    expect_error("<?php echo (1 + 2;", "Expected closing ')'");
}

#[test]
fn test_error_unexpected_token_in_expr() {
    expect_error("<?php echo ;", "Unexpected token");
}

#[test]
fn test_error_unexpected_token_in_stmt() {
    expect_error("<?php 42;", "Unexpected token");
}

#[test]
fn test_error_try_requires_catch_or_finally() {
    expect_error("<?php try { echo 1; }", "Expected at least one catch or a finally block after try");
}

#[test]
fn test_error_throw_requires_object() {
    expect_error("<?php throw 123;", "throw requires an object value");
}

#[test]
fn test_error_throw_requires_throwable() {
    expect_error(
        "<?php class PlainObject {} throw new PlainObject();",
        "throw requires an object implementing Throwable",
    );
}

#[test]
fn test_error_throw_expression_requires_object() {
    expect_error(
        "<?php $value = null ?? throw 123;",
        "throw requires an object value",
    );
}

#[test]
fn test_error_string_index_requires_integer() {
    expect_error(
        "<?php $s = \"hello\"; echo $s[\"x\"];",
        "String index must be integer",
    );
}

#[test]
fn test_error_string_offset_assignment_is_not_supported() {
    expect_error(
        "<?php $s = \"hello\"; $s[0] = \"H\";",
        "String offset assignment is not supported",
    );
}

#[test]
fn test_error_magic_tostring_must_be_public() {
    expect_error(
        "<?php class User { private function __toString() { return \"x\"; } }",
        "Magic method must be public: User::__toString",
    );
}

#[test]
fn test_error_magic_tostring_must_take_zero_arguments() {
    expect_error(
        "<?php class User { public function __toString($x) { return \"x\"; } }",
        "Magic method must take 0 arguments: User::__toString",
    );
}

#[test]
fn test_error_magic_tostring_must_return_string() {
    expect_error(
        "<?php class User { public function __toString() { return 123; } }",
        "Magic method must return string: User::__toString",
    );
}

#[test]
fn test_error_magic_get_must_take_one_argument() {
    expect_error(
        "<?php class Bag { public function __get() { return 1; } }",
        "Magic method must take 1 argument: Bag::__get",
    );
}

#[test]
fn test_error_magic_set_must_be_public() {
    expect_error(
        "<?php class Bag { private function __set($name, $value) { } }",
        "Magic method must be public: Bag::__set",
    );
}

#[test]
fn test_error_magic_set_must_take_two_arguments() {
    expect_error(
        "<?php class Bag { public function __set($name) { } }",
        "Magic method must take 2 arguments: Bag::__set",
    );
}

#[test]
fn test_error_catch_requires_defined_class() {
    expect_error(
        "<?php try { echo 1; } catch (MissingException $e) { echo 2; }",
        "Undefined class: MissingException",
    );
}

#[test]
fn test_error_catch_requires_throwable_type() {
    expect_error(
        "<?php class PlainObject {} try { throw new Exception(); } catch (PlainObject $e) { echo 2; }",
        "Catch type must extend or implement Throwable: PlainObject",
    );
}

#[test]
fn test_error_duplicate_use_alias_is_rejected() {
    expect_error(
        "<?php namespace App; use Lib\\One as Tool; use Lib\\Two as Tool; echo 1;",
        "Duplicate import alias: Tool",
    );
}

#[test]
fn test_error_packed_class_rejects_non_pod_field() {
    expect_error(
        "<?php packed class Bad { public string $name; }",
        "Packed class fields must use POD scalars, pointers, or packed classes",
    );
}

#[test]
fn test_error_typed_local_decl_currently_requires_buffer() {
    expect_error(
        "<?php int $value = 1;",
        "Typed local declarations currently support only buffer<T>",
    );
}

#[test]
fn test_error_buffer_new_rejects_non_pod_element_type() {
    expect_error(
        "<?php buffer<string> $names = buffer_new<string>(2);",
        "buffer<T> requires a POD scalar, pointer, or packed class element type",
    );
}

#[test]
fn test_error_buffer_scalar_assign_type_mismatch() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<int>(2); $values[0] = true;",
        "Buffer element type mismatch",
    );
}

#[test]
fn test_error_buffer_packed_element_requires_field_assignment() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(1); $points[0] = 1;",
        "Assign packed buffer elements through field access like $buf[$i]->field",
    );
}

#[test]
fn test_error_buffer_len_requires_buffer_argument() {
    expect_error(
        "<?php echo buffer_len(1);",
        "buffer_len() argument must be buffer<T>",
    );
}

#[test]
fn test_error_buffer_free_requires_buffer_argument() {
    expect_error(
        "<?php buffer_free(42);",
        "buffer_free() argument must be buffer<T>",
    );
}

#[test]
fn test_error_buffer_free_wrong_arg_count() {
    expect_error(
        "<?php buffer<int> $b = buffer_new<int>(1); buffer_free($b, $b);",
        "buffer_free() takes exactly 1 argument",
    );
}

#[test]
fn test_error_buffer_free_requires_local_variable() {
    expect_error(
        "<?php buffer_free(buffer_new<int>(1));",
        "buffer_free() argument must be a local variable",
    );
}

#[test]
fn test_error_buffer_free_rejects_ref_param() {
    expect_error(
        "<?php function drop(&$buf) { buffer_free($buf); } buffer<int> $buf = buffer_new<int>(1); drop($buf);",
        "buffer_free() argument must be a local variable",
    );
}

#[test]
fn test_error_buffer_free_rejects_global_alias() {
    expect_error(
        "<?php buffer<int> $buf = buffer_new<int>(1); function drop() { global $buf; buffer_free($buf); } drop();",
        "buffer_free() argument must be a local variable",
    );
}

#[test]
fn test_error_buffer_free_rejects_static_slot() {
    expect_error(
        "<?php function drop() { static $buf = buffer_new<int>(1); buffer_free($buf); } drop();",
        "buffer_free() argument must be a local variable",
    );
}

#[test]
fn test_error_cannot_redeclare_builtin_exception_type() {
    expect_error(
        "<?php class Exception {}",
        "Cannot redeclare built-in exception type: Exception",
    );
}

#[test]
fn test_error_cannot_instantiate_throwable_interface() {
    expect_error(
        "<?php $e = new Throwable();",
        "Cannot instantiate interface: Throwable",
    );
}

#[test]
fn test_error_missing_function_name() {
    expect_error("<?php function () { }", "Expected function name");
}

#[test]
fn test_error_missing_function_paren() {
    expect_error("<?php function foo { }", "Expected '(' after function name");
}

#[test]
fn test_error_missing_if_paren() {
    expect_error("<?php if 1 { }", "Expected '(' after 'if'");
}

#[test]
fn test_error_ifdef_requires_symbol_name() {
    expect_error("<?php ifdef { echo 1; }", "Expected symbol name after 'ifdef'");
}

#[test]
fn test_error_ifdef_requires_braced_body() {
    expect_error("<?php ifdef DEBUG echo 1;", "Expected '{'");
}

#[test]
fn test_error_missing_while_paren() {
    expect_error("<?php while 1 { }", "Expected '(' after 'while'");
}

// --- Type errors ---

#[test]
fn test_error_undefined_variable() {
    expect_error("<?php echo $x;", "Undefined variable: $x");
}

#[test]
fn test_error_type_mismatch_reassign() {
    expect_error("<?php $x = 42; $x = \"hello\";", "cannot reassign $x");
}

#[test]
fn test_error_arithmetic_on_string() {
    expect_error(
        "<?php $x = \"hi\"; echo $x + 1;",
        "Arithmetic operators require numeric operands",
    );
}

#[test]
fn test_error_negate_string() {
    expect_error(
        "<?php $x = \"hi\"; echo -$x;",
        "Cannot negate a non-numeric value",
    );
}

#[test]
fn test_error_comparison_on_string() {
    expect_error(
        "<?php $x = \"a\"; echo $x < 1;",
        "Comparison operators require numeric operands",
    );
}

#[test]
fn test_error_undefined_function() {
    expect_error("<?php nope();", "Undefined function: nope");
}

#[test]
fn test_error_wrong_arg_count() {
    expect_error(
        "<?php function f($a) { return $a; } f(1, 2);",
        "expects 1 arguments, got 2",
    );
}

#[test]
fn test_error_trait_method_conflict_requires_insteadof() {
    expect_error(
        r#"<?php
trait A { public function foo() { return 1; } }
trait B { public function foo() { return 2; } }
class C { use A, B; }
"#,
        "ambiguous trait method 'foo'",
    );
}

#[test]
fn test_error_trait_property_conflict_must_be_compatible() {
    expect_error(
        r#"<?php
trait A { public $value = 1; }
trait B { private $value = 1; }
class C { use A, B; }
"#,
        "incompatible duplicate property",
    );
}

#[test]
fn test_error_cannot_access_protected_trait_method_outside_class() {
    expect_error(
        r#"<?php
trait A { public function foo() { return 1; } }
class C { use A { A::foo as protected; } }
$c = new C();
echo $c->foo();
"#,
        "Cannot access protected method",
    );
}

#[test]
fn test_error_circular_trait_composition() {
    expect_error(
        r#"<?php
trait A { use B; }
trait B { use A; }
class C { use A; }
"#,
        "Circular trait composition detected",
    );
}

#[test]
fn test_error_cannot_access_protected_property_outside_class() {
    expect_error(
        r#"<?php
class Secret {
    protected $value = 7;
}
$s = new Secret();
echo $s->value;
"#,
        "Cannot access protected property: Secret::value",
    );
}

#[test]
fn test_error_cannot_access_protected_method_outside_class() {
    expect_error(
        r#"<?php
class Secret {
    protected function hidden() {
        return 7;
    }
}
$s = new Secret();
echo $s->hidden();
"#,
        "Cannot access protected method: Secret::hidden",
    );
}

#[test]
fn test_error_increment_string() {
    expect_error("<?php $x = \"hi\"; $x++;", "Cannot increment/decrement");
}

// --- Error positions ---

#[test]
fn test_error_has_line_number() {
    let result = tokenize("<?php\n\n\"unterminated");
    let err = result.unwrap_err();
    assert_eq!(err.span.line, 3, "Error should be on line 3");
}

#[test]
fn test_error_has_column() {
    let result = tokenize("<?php @");
    let err = result.unwrap_err();
    assert!(err.span.col > 0, "Error should have a column number");
}

// --- Float/math function errors ---

#[test]
fn test_error_floor_wrong_args() {
    expect_error("<?php floor(1, 2);", "floor() takes exactly 1 argument");
}

#[test]
fn test_error_ceil_wrong_args() {
    expect_error("<?php ceil();", "ceil() takes exactly 1 argument");
}

#[test]
fn test_error_round_wrong_args() {
    expect_error("<?php round();", "round() takes 1 or 2 arguments");
}

#[test]
fn test_error_sqrt_wrong_args() {
    expect_error("<?php sqrt(1, 2);", "sqrt() takes exactly 1 argument");
}

#[test]
fn test_error_pow_wrong_args() {
    expect_error("<?php pow(1);", "pow() takes exactly 2 arguments");
}

#[test]
fn test_error_min_wrong_args() {
    expect_error("<?php min(1);", "min() requires at least 2 arguments");
}

#[test]
fn test_error_max_wrong_args() {
    expect_error("<?php max(1);", "max() requires at least 2 arguments");
}

#[test]
fn test_error_intdiv_wrong_args() {
    expect_error("<?php intdiv(1);", "intdiv() takes exactly 2 arguments");
}

#[test]
fn test_error_abs_wrong_args() {
    expect_error("<?php abs();", "abs() takes exactly 1 argument");
}

#[test]
fn test_error_floatval_wrong_args() {
    expect_error("<?php floatval();", "floatval() takes exactly 1 argument");
}

#[test]
fn test_error_is_float_wrong_args() {
    expect_error("<?php is_float();", "is_float() takes exactly 1 argument");
}

#[test]
fn test_error_is_int_wrong_args() {
    expect_error("<?php is_int();", "is_int() takes exactly 1 argument");
}

expect_builtin_arity_error!(
    test_error_strlen_wrong_args,
    "<?php strlen();",
    "strlen() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_intval_wrong_args,
    "<?php intval();",
    "intval() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_strrpos_wrong_args,
    "<?php strrpos(\"abc\");",
    "strrpos() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_strstr_wrong_args,
    "<?php strstr(\"abc\");",
    "strstr() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_strtolower_wrong_args,
    "<?php strtolower();",
    "strtolower() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_strtoupper_wrong_args,
    "<?php strtoupper();",
    "strtoupper() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_ucfirst_wrong_args,
    "<?php ucfirst();",
    "ucfirst() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_lcfirst_wrong_args,
    "<?php lcfirst();",
    "lcfirst() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_trim_wrong_args,
    "<?php trim(\"x\", \"y\", \"z\");",
    "trim() takes 1 or 2 arguments"
);
expect_builtin_arity_error!(
    test_error_ltrim_wrong_args,
    "<?php ltrim(\"x\", \"y\", \"z\");",
    "ltrim() takes 1 or 2 arguments"
);
expect_builtin_arity_error!(
    test_error_rtrim_wrong_args,
    "<?php rtrim(\"x\", \"y\", \"z\");",
    "rtrim() takes 1 or 2 arguments"
);
expect_builtin_arity_error!(
    test_error_str_repeat_wrong_args,
    "<?php str_repeat(\"x\");",
    "str_repeat() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_strrev_wrong_args,
    "<?php strrev();",
    "strrev() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_chr_wrong_args,
    "<?php chr();",
    "chr() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_strcmp_wrong_args,
    "<?php strcmp(\"a\");",
    "strcmp() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_strcasecmp_wrong_args,
    "<?php strcasecmp(\"a\");",
    "strcasecmp() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_str_contains_wrong_args,
    "<?php str_contains(\"a\");",
    "str_contains() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_str_starts_with_wrong_args,
    "<?php str_starts_with(\"a\");",
    "str_starts_with() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_str_ends_with_wrong_args,
    "<?php str_ends_with(\"a\");",
    "str_ends_with() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_implode_wrong_args,
    "<?php implode([\"a\"]);",
    "implode() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_ucwords_wrong_args,
    "<?php ucwords();",
    "ucwords() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_str_ireplace_wrong_args,
    "<?php str_ireplace(\"a\", \"b\");",
    "str_ireplace() takes exactly 3 arguments"
);
expect_builtin_arity_error!(
    test_error_substr_replace_wrong_args,
    "<?php substr_replace(\"abc\", \"x\");",
    "substr_replace() takes 3 or 4 arguments"
);
expect_builtin_arity_error!(
    test_error_str_split_wrong_args,
    "<?php str_split(\"abc\", 1, 2);",
    "str_split() takes 1 or 2 arguments"
);
expect_builtin_arity_error!(
    test_error_addslashes_wrong_args,
    "<?php addslashes();",
    "addslashes() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_stripslashes_wrong_args,
    "<?php stripslashes();",
    "stripslashes() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_nl2br_wrong_args,
    "<?php nl2br();",
    "nl2br() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_wordwrap_wrong_args,
    "<?php wordwrap(\"a\", 1, \"-\", true, 5);",
    "wordwrap() takes 1 to 4 arguments"
);
expect_builtin_arity_error!(
    test_error_bin2hex_wrong_args,
    "<?php bin2hex();",
    "bin2hex() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_hex2bin_wrong_args,
    "<?php hex2bin();",
    "hex2bin() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_htmlentities_wrong_args,
    "<?php htmlentities();",
    "htmlentities() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_html_entity_decode_wrong_args,
    "<?php html_entity_decode();",
    "html_entity_decode() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_urldecode_wrong_args,
    "<?php urldecode();",
    "urldecode() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_rawurlencode_wrong_args,
    "<?php rawurlencode();",
    "rawurlencode() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_rawurldecode_wrong_args,
    "<?php rawurldecode();",
    "rawurldecode() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_base64_decode_wrong_args,
    "<?php base64_decode();",
    "base64_decode() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_ctype_digit_wrong_args,
    "<?php ctype_digit();",
    "ctype_digit() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_ctype_alnum_wrong_args,
    "<?php ctype_alnum();",
    "ctype_alnum() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_ctype_space_wrong_args,
    "<?php ctype_space();",
    "ctype_space() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_is_bool_wrong_args,
    "<?php is_bool();",
    "is_bool() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_boolval_wrong_args,
    "<?php boolval();",
    "boolval() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_is_string_wrong_args,
    "<?php is_string();",
    "is_string() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_is_numeric_wrong_args,
    "<?php is_numeric();",
    "is_numeric() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_fdiv_wrong_args,
    "<?php fdiv(1);",
    "fdiv() takes exactly 2 arguments"
);
expect_builtin_arity_error!(
    test_error_mt_rand_wrong_args,
    "<?php mt_rand(1);",
    "mt_rand() takes 0 or 2 arguments"
);
expect_builtin_arity_error!(
    test_error_rand_wrong_args,
    "<?php rand(1);",
    "rand() takes 0 or 2 arguments"
);
expect_builtin_arity_error!(
    test_error_asin_wrong_args,
    "<?php asin();",
    "asin() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_acos_wrong_args,
    "<?php acos();",
    "acos() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_sinh_wrong_args,
    "<?php sinh();",
    "sinh() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_cosh_wrong_args,
    "<?php cosh();",
    "cosh() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_tanh_wrong_args,
    "<?php tanh();",
    "tanh() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_log2_wrong_args,
    "<?php log2();",
    "log2() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_log10_wrong_args,
    "<?php log10();",
    "log10() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_rad2deg_wrong_args,
    "<?php rad2deg();",
    "rad2deg() takes exactly 1 argument"
);
expect_builtin_arity_error!(
    test_error_exit_wrong_args,
    "<?php exit(1, 2);",
    "exit() takes 0 or 1 arguments"
);
expect_builtin_arity_error!(
    test_error_die_wrong_args,
    "<?php die(1, 2);",
    "exit() takes 0 or 1 arguments"
);

#[test]
fn test_null_coalesce_widens_function_return_type_in_checker() {
    let tokens = tokenize("<?php function fallback_pi($x) { return $x ?? 3.14159; }")
        .expect("tokenize failed");
    let ast = parse(&tokens).expect("parse failed");
    let check_result = types::check(&ast).expect("type check failed");

    let sig = check_result
        .functions
        .get("fallback_pi")
        .expect("missing function signature for fallback_pi");
    assert_eq!(sig.return_type, PhpType::Float);
}

// --- Include/Require errors ---

#[test]
fn test_error_include_missing_path() {
    expect_error("<?php include ;", "Expected string path");
}

#[test]
fn test_error_include_non_string_path() {
    expect_error("<?php include 42;", "Expected string path");
}

// --- INF/NAN function errors ---

#[test]
fn test_error_is_nan_wrong_args() {
    expect_error("<?php is_nan();", "is_nan() takes exactly 1 argument");
}

#[test]
fn test_error_is_finite_wrong_args() {
    expect_error("<?php is_finite();", "is_finite() takes exactly 1 argument");
}

#[test]
fn test_error_is_infinite_wrong_args() {
    expect_error(
        "<?php is_infinite();",
        "is_infinite() takes exactly 1 argument",
    );
}

// --- Type operation errors ---

#[test]
fn test_error_gettype_wrong_args() {
    expect_error("<?php gettype();", "gettype() takes exactly 1 argument");
}

#[test]
fn test_error_empty_wrong_args() {
    expect_error("<?php empty();", "empty() takes exactly 1 argument");
}

#[test]
fn test_error_unset_wrong_args() {
    expect_error("<?php unset();", "unset() takes exactly 1 argument");
}

#[test]
fn test_error_settype_wrong_args() {
    expect_error("<?php settype(42);", "settype() takes exactly 2 arguments");
}

#[test]
fn test_error_fmod_wrong_args() {
    expect_error("<?php fmod(1);", "fmod() takes exactly 2 arguments");
}

#[test]
fn test_error_random_int_wrong_args() {
    expect_error(
        "<?php random_int(1);",
        "random_int() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_number_format_wrong_args() {
    expect_error(
        "<?php number_format();",
        "number_format() takes 1 to 4 arguments",
    );
}

// --- String function errors ---

#[test]
fn test_error_substr_wrong_args() {
    expect_error("<?php substr(\"hi\");", "substr() takes 2 or 3 arguments");
}

#[test]
fn test_error_strpos_wrong_args() {
    expect_error(
        "<?php strpos(\"hi\");",
        "strpos() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_str_replace_wrong_args() {
    expect_error(
        "<?php str_replace(\"a\", \"b\");",
        "str_replace() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_sprintf_no_args() {
    expect_error("<?php sprintf();", "sprintf() requires at least 1 argument");
}

#[test]
fn test_error_printf_no_args() {
    expect_error("<?php printf();", "printf() requires at least 1 argument");
}

#[test]
fn test_error_ord_wrong_args() {
    expect_error("<?php ord();", "ord() takes exactly 1 argument");
}

#[test]
fn test_error_explode_wrong_args() {
    expect_error(
        "<?php explode(\",\");",
        "explode() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_str_pad_wrong_args() {
    expect_error("<?php str_pad(\"x\");", "str_pad() takes 2 to 4 arguments");
}

#[test]
fn test_error_md5_wrong_args() {
    expect_error("<?php md5();", "md5() takes exactly 1 argument");
}

#[test]
fn test_error_sha1_wrong_args() {
    expect_error("<?php sha1();", "sha1() takes exactly 1 argument");
}

#[test]
fn test_error_htmlspecialchars_wrong_args() {
    expect_error(
        "<?php htmlspecialchars();",
        "htmlspecialchars() takes exactly 1 argument",
    );
}

#[test]
fn test_error_urlencode_wrong_args() {
    expect_error("<?php urlencode();", "urlencode() takes exactly 1 argument");
}

#[test]
fn test_error_base64_encode_wrong_args() {
    expect_error(
        "<?php base64_encode();",
        "base64_encode() takes exactly 1 argument",
    );
}

#[test]
fn test_error_ctype_alpha_wrong_args() {
    expect_error(
        "<?php ctype_alpha();",
        "ctype_alpha() takes exactly 1 argument",
    );
}

#[test]
fn test_error_hash_wrong_args() {
    expect_error(r#"<?php hash("md5");"#, "hash() takes exactly 2 arguments");
}

#[test]
fn test_error_sscanf_wrong_args() {
    expect_error(
        r#"<?php sscanf("hi");"#,
        "sscanf() takes at least 2 arguments",
    );
}

// --- v0.5: I/O function errors ---

#[test]
fn test_error_var_dump_wrong_args() {
    expect_error("<?php var_dump();", "var_dump() takes exactly 1 argument");
}

#[test]
fn test_error_print_r_wrong_args() {
    expect_error("<?php print_r();", "print_r() takes exactly 1 argument");
}

#[test]
fn test_error_fopen_wrong_args() {
    expect_error(
        r#"<?php fopen("file");"#,
        "fopen() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_fclose_wrong_args() {
    expect_error("<?php fclose();", "fclose() takes exactly 1 argument");
}

#[test]
fn test_error_fread_wrong_args() {
    expect_error("<?php fread(1);", "fread() takes exactly 2 arguments");
}

#[test]
fn test_error_fwrite_wrong_args() {
    expect_error("<?php fwrite(1);", "fwrite() takes exactly 2 arguments");
}

#[test]
fn test_error_fgets_wrong_args() {
    expect_error("<?php fgets();", "fgets() takes exactly 1 argument");
}

#[test]
fn test_error_feof_wrong_args() {
    expect_error("<?php feof();", "feof() takes exactly 1 argument");
}

#[test]
fn test_error_file_get_contents_wrong_args() {
    expect_error(
        "<?php file_get_contents();",
        "file_get_contents() takes exactly 1 argument",
    );
}

#[test]
fn test_error_file_put_contents_wrong_args() {
    expect_error(
        r#"<?php file_put_contents("x");"#,
        "file_put_contents() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_file_exists_wrong_args() {
    expect_error(
        "<?php file_exists();",
        "file_exists() takes exactly 1 argument",
    );
}

#[test]
fn test_error_mkdir_wrong_args() {
    expect_error("<?php mkdir();", "mkdir() takes exactly 1 argument");
}

#[test]
fn test_error_copy_wrong_args() {
    expect_error(r#"<?php copy("x");"#, "copy() takes exactly 2 arguments");
}

#[test]
fn test_error_rename_wrong_args() {
    expect_error(
        r#"<?php rename("x");"#,
        "rename() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_getcwd_wrong_args() {
    expect_error("<?php getcwd(1);", "getcwd() takes no arguments");
}

#[test]
fn test_error_scandir_wrong_args() {
    expect_error("<?php scandir();", "scandir() takes exactly 1 argument");
}

#[test]
fn test_error_tempnam_wrong_args() {
    expect_error(
        r#"<?php tempnam("x");"#,
        "tempnam() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_is_file_wrong_args() {
    expect_error("<?php is_file();", "is_file() takes exactly 1 argument");
}

#[test]
fn test_error_is_dir_wrong_args() {
    expect_error("<?php is_dir();", "is_dir() takes exactly 1 argument");
}

#[test]
fn test_error_is_readable_wrong_args() {
    expect_error(
        "<?php is_readable();",
        "is_readable() takes exactly 1 argument",
    );
}

#[test]
fn test_error_is_writable_wrong_args() {
    expect_error(
        "<?php is_writable();",
        "is_writable() takes exactly 1 argument",
    );
}

#[test]
fn test_error_filesize_wrong_args() {
    expect_error("<?php filesize();", "filesize() takes exactly 1 argument");
}

#[test]
fn test_error_filemtime_wrong_args() {
    expect_error("<?php filemtime();", "filemtime() takes exactly 1 argument");
}

#[test]
fn test_error_unlink_wrong_args() {
    expect_error("<?php unlink();", "unlink() takes exactly 1 argument");
}

#[test]
fn test_error_rmdir_wrong_args() {
    expect_error("<?php rmdir();", "rmdir() takes exactly 1 argument");
}

#[test]
fn test_error_chdir_wrong_args() {
    expect_error("<?php chdir();", "chdir() takes exactly 1 argument");
}

#[test]
fn test_error_glob_wrong_args() {
    expect_error("<?php glob();", "glob() takes exactly 1 argument");
}

#[test]
fn test_error_sys_get_temp_dir_wrong_args() {
    expect_error(
        "<?php sys_get_temp_dir(1);",
        "sys_get_temp_dir() takes no arguments",
    );
}

#[test]
fn test_error_rewind_wrong_args() {
    expect_error("<?php rewind();", "rewind() takes exactly 1 argument");
}

#[test]
fn test_error_ftell_wrong_args() {
    expect_error("<?php ftell();", "ftell() takes exactly 1 argument");
}

#[test]
fn test_error_fseek_wrong_args() {
    expect_error("<?php fseek(1);", "fseek() takes 2 or 3 arguments");
}

#[test]
fn test_error_file_wrong_args() {
    expect_error("<?php file();", "file() takes exactly 1 argument");
}

#[test]
fn test_error_readline_wrong_args() {
    expect_error(
        r#"<?php readline(1, 2);"#,
        "readline() takes 0 or 1 arguments",
    );
}

#[test]
fn test_error_fgetcsv_wrong_args() {
    expect_error("<?php fgetcsv();", "fgetcsv() takes 1 to 3 arguments");
}

#[test]
fn test_error_fputcsv_wrong_args() {
    expect_error("<?php fputcsv(1);", "fputcsv() takes 2 to 4 arguments");
}

// --- v0.6: switch/match/array errors ---

#[test]
fn test_error_switch_missing_paren() {
    expect_error("<?php switch $x {}", "Expected '(' after 'switch'");
}

#[test]
fn test_error_match_missing_paren() {
    expect_error("<?php $x = match $x {};", "Expected '(' after 'match'");
}

#[test]
fn test_assoc_array_mixed_type_checks() {
    assert!(
        check_source(r#"<?php $a = ["name" => "Alice", "age" => 30];"#).is_ok(),
        "heterogeneous associative-array values should widen to mixed",
    );
}

// --- v0.6: array function argument errors ---

#[test]
fn test_error_array_reverse_wrong_args() {
    expect_error(
        "<?php array_reverse();",
        "array_reverse() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_merge_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_merge($a);",
        "array_merge() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_sum_wrong_args() {
    expect_error("<?php array_sum();", "array_sum() takes exactly 1 argument");
}

#[test]
fn test_error_array_search_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_search($a);",
        "array_search() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_key_exists_wrong_args() {
    expect_error(
        "<?php array_key_exists(1);",
        "array_key_exists() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_slice_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_slice($a);",
        "array_slice() takes 2 or 3 arguments",
    );
}

#[test]
fn test_error_array_combine_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_combine($a);",
        "array_combine() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_range_wrong_args() {
    expect_error("<?php range(1);", "range() takes exactly 2 arguments");
}

#[test]
fn test_error_shuffle_wrong_args() {
    expect_error("<?php shuffle();", "shuffle() takes exactly 1 argument");
}

#[test]
fn test_error_array_fill_wrong_args() {
    expect_error(
        "<?php array_fill(0, 5);",
        "array_fill() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_array_push_wrong_args() {
    expect_error(
        "<?php array_push();",
        "array_push() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_pop_wrong_args() {
    expect_error("<?php array_pop();", "array_pop() takes exactly 1 argument");
}

#[test]
fn test_error_in_array_wrong_args() {
    expect_error("<?php in_array(1);", "in_array() takes exactly 2 arguments");
}

#[test]
fn test_error_array_keys_wrong_args() {
    expect_error(
        "<?php array_keys();",
        "array_keys() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_values_wrong_args() {
    expect_error(
        "<?php array_values();",
        "array_values() takes exactly 1 argument",
    );
}

#[test]
fn test_error_sort_wrong_args() {
    expect_error("<?php sort();", "sort() takes exactly 1 argument");
}

#[test]
fn test_error_rsort_wrong_args() {
    expect_error("<?php rsort();", "rsort() takes exactly 1 argument");
}

#[test]
fn test_error_isset_wrong_args() {
    expect_error("<?php isset();", "isset() takes exactly 1 argument");
}

#[test]
fn test_error_array_unique_wrong_args() {
    expect_error(
        "<?php array_unique();",
        "array_unique() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_product_wrong_args() {
    expect_error(
        "<?php array_product();",
        "array_product() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_shift_wrong_args() {
    expect_error(
        "<?php array_shift();",
        "array_shift() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_unshift_wrong_args() {
    expect_error(
        "<?php array_unshift();",
        "array_unshift() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_splice_wrong_args() {
    expect_error(
        "<?php array_splice();",
        "array_splice() takes 2 or 3 arguments",
    );
}

#[test]
fn test_error_array_flip_wrong_args() {
    expect_error(
        "<?php array_flip();",
        "array_flip() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_chunk_wrong_args() {
    expect_error(
        "<?php array_chunk();",
        "array_chunk() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_pad_wrong_args() {
    expect_error(
        "<?php array_pad();",
        "array_pad() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_array_fill_keys_wrong_args() {
    expect_error(
        "<?php array_fill_keys();",
        "array_fill_keys() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_count_wrong_args() {
    expect_error("<?php count();", "count() takes exactly 1 argument");
}

#[test]
fn test_error_array_diff_wrong_args() {
    expect_error(
        "<?php array_diff();",
        "array_diff() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_intersect_wrong_args() {
    expect_error(
        "<?php array_intersect();",
        "array_intersect() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_diff_key_wrong_args() {
    expect_error(
        "<?php array_diff_key();",
        "array_diff_key() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_intersect_key_wrong_args() {
    expect_error(
        "<?php array_intersect_key();",
        "array_intersect_key() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_rand_wrong_args() {
    expect_error(
        "<?php array_rand();",
        "array_rand() takes exactly 1 argument",
    );
}

#[test]
fn test_error_asort_wrong_args() {
    expect_error("<?php asort();", "asort() takes exactly 1 argument");
}

#[test]
fn test_error_arsort_wrong_args() {
    expect_error("<?php arsort();", "arsort() takes exactly 1 argument");
}

#[test]
fn test_error_ksort_wrong_args() {
    expect_error("<?php ksort();", "ksort() takes exactly 1 argument");
}

#[test]
fn test_error_krsort_wrong_args() {
    expect_error("<?php krsort();", "krsort() takes exactly 1 argument");
}

#[test]
fn test_error_natsort_wrong_args() {
    expect_error("<?php natsort();", "natsort() takes exactly 1 argument");
}

#[test]
fn test_error_natcasesort_wrong_args() {
    expect_error(
        "<?php natcasesort();",
        "natcasesort() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_column_wrong_args() {
    expect_error(
        r#"<?php array_column([]);"#,
        "array_column() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_map_wrong_args() {
    expect_error(
        r#"<?php array_map("fn");"#,
        "array_map() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_filter_wrong_args() {
    expect_error(
        r#"<?php array_filter([]);"#,
        "array_filter() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_reduce_wrong_args() {
    expect_error(
        r#"<?php array_reduce([], "fn");"#,
        "array_reduce() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_array_walk_wrong_args() {
    expect_error(
        r#"<?php array_walk([]);"#,
        "array_walk() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_usort_wrong_args() {
    expect_error(r#"<?php usort([]);"#, "usort() takes exactly 2 arguments");
}

#[test]
fn test_error_uksort_wrong_args() {
    expect_error(r#"<?php uksort([]);"#, "uksort() takes exactly 2 arguments");
}

#[test]
fn test_error_uasort_wrong_args() {
    expect_error(r#"<?php uasort([]);"#, "uasort() takes exactly 2 arguments");
}

#[test]
fn test_error_call_user_func_wrong_args() {
    expect_error(
        r#"<?php call_user_func();"#,
        "call_user_func() takes at least 1 argument",
    );
}

#[test]
fn test_error_function_exists_wrong_args() {
    expect_error(
        r#"<?php function_exists();"#,
        "function_exists() takes exactly 1 argument",
    );
}

// --- Closure / arrow function errors ---

#[test]
fn test_error_call_non_callable_variable() {
    expect_error(r#"<?php $x = 5; $x(1);"#, "not a callable");
}

#[test]
fn test_error_arrow_function_missing_arrow() {
    expect_error(r#"<?php $f = fn($x) $x * 2;"#, "Expected '=>'");
}

#[test]
fn test_error_arrow_function_missing_lparen() {
    expect_error(r#"<?php $f = fn $x => $x * 2;"#, "Expected '(' after 'fn'");
}

// --- v0.7: Default parameter, bitwise, spaceship errors ---

#[test]
fn test_error_too_many_args_with_defaults() {
    expect_error(
        "<?php function f($a, $b = 1) { return $a + $b; } f(1, 2, 3);",
        "expects 1 to 2 arguments, got 3",
    );
}

#[test]
fn test_error_too_few_args_with_defaults() {
    expect_error(
        "<?php function f($a, $b = 1) { return $a + $b; } f();",
        "expects 1 to 2 arguments, got 0",
    );
}

#[test]
fn test_error_bitwise_and_string() {
    expect_error(
        r#"<?php echo "hello" & 1;"#,
        "Bitwise operators require integer operands",
    );
}

#[test]
fn test_error_bitwise_not_string() {
    expect_error(
        r#"<?php echo ~"hello";"#,
        "Bitwise NOT requires integer operand",
    );
}

#[test]
fn test_error_spaceship_string() {
    expect_error(
        r#"<?php echo "a" <=> "b";"#,
        "Spaceship operator requires numeric operands",
    );
}

#[test]
fn test_error_heredoc_unterminated() {
    expect_error("<?php echo <<<EOT\nHello", "Unterminated heredoc");
}

// --- Constants errors ---

#[test]
fn test_error_undefined_constant() {
    expect_error("<?php echo UNDEFINED_CONST;", "Undefined constant");
}

#[test]
fn test_error_const_missing_name() {
    expect_error("<?php const = 5;", "Expected constant name");
}

#[test]
fn test_error_const_missing_value() {
    expect_error("<?php const MAX;", "Expected '='");
}

#[test]
fn test_error_define_wrong_args() {
    expect_error("<?php define(\"X\");", "define() takes exactly 2 arguments");
}

#[test]
fn test_error_define_non_string_name() {
    expect_error(
        "<?php define(42, 100);",
        "define() first argument must be a string literal",
    );
}

// --- List unpack errors ---

#[test]
fn test_error_list_unpack_non_array() {
    expect_error("<?php [$a, $b] = 42;", "List unpacking requires an array");
}

// --- call_user_func_array errors ---

#[test]
fn test_error_call_user_func_array_wrong_args() {
    expect_error(
        "<?php call_user_func_array(\"foo\");",
        "call_user_func_array() takes exactly 2 arguments",
    );
}

// --- v0.8 system function errors ---

#[test]
fn test_error_time_wrong_args() {
    expect_error("<?php time(1);", "time() takes no arguments");
}

#[test]
fn test_error_microtime_wrong_args() {
    expect_error(
        "<?php microtime(1, 2);",
        "microtime() takes 0 or 1 arguments",
    );
}

#[test]
fn test_error_sleep_wrong_args() {
    expect_error("<?php sleep();", "sleep() takes exactly 1 argument");
}

#[test]
fn test_error_usleep_wrong_args() {
    expect_error("<?php usleep();", "usleep() takes exactly 1 argument");
}

#[test]
fn test_error_getenv_wrong_args() {
    expect_error("<?php getenv();", "getenv() takes exactly 1 argument");
}

#[test]
fn test_error_putenv_wrong_args() {
    expect_error("<?php putenv();", "putenv() takes exactly 1 argument");
}

#[test]
fn test_error_phpversion_wrong_args() {
    expect_error("<?php phpversion(1);", "phpversion() takes no arguments");
}

#[test]
fn test_error_php_uname_wrong_args() {
    expect_error(
        "<?php php_uname(1, 2);",
        "php_uname() takes 0 or 1 arguments",
    );
}

#[test]
fn test_error_exec_wrong_args() {
    expect_error("<?php exec();", "exec() takes exactly 1 argument");
}

#[test]
fn test_error_shell_exec_wrong_args() {
    expect_error(
        "<?php shell_exec();",
        "shell_exec() takes exactly 1 argument",
    );
}

#[test]
fn test_error_system_wrong_args() {
    expect_error("<?php system();", "system() takes exactly 1 argument");
}

#[test]
fn test_error_passthru_wrong_args() {
    expect_error("<?php passthru();", "passthru() takes exactly 1 argument");
}

// --- Global/Static parse errors ---

#[test]
fn test_error_global_missing_var() {
    expect_error("<?php global ;", "Expected variable after 'global'");
}

#[test]
fn test_error_static_missing_var() {
    expect_error("<?php static ;", "Expected variable after 'static'");
}

#[test]
fn test_error_static_missing_init() {
    expect_error("<?php static $x;", "Expected '=' after static variable");
}

// --- Variadic / Spread errors ---

#[test]
fn test_error_variadic_missing_variable() {
    expect_error(
        "<?php function foo(... ) {}",
        "Expected variable after '...'",
    );
}

#[test]
fn test_error_variadic_not_last() {
    expect_error(
        "<?php function foo(...$a, $b) {}",
        "Variadic parameter must be the last parameter",
    );
}

#[test]
fn test_error_spread_non_array() {
    expect_error(
        "<?php $x = 5; $y = [...$x];",
        "Spread operator requires an array",
    );
}

#[test]
fn test_error_undefined_class() {
    expect_error("<?php $x = new Missing();", "Undefined class: Missing");
}

#[test]
fn test_error_undefined_property() {
    expect_error(
        "<?php class Box {} $b = new Box(); echo $b->missing;",
        "Undefined property: Box::missing",
    );
}

#[test]
fn test_error_undefined_method() {
    expect_error(
        "<?php class Box {} $b = new Box(); $b->missing();",
        "Undefined method: Box::missing",
    );
}

#[test]
fn test_error_private_access() {
    expect_error(
        "<?php class Secret { private $value = 7; } $s = new Secret(); echo $s->value;",
        "Cannot access private property: Secret::value",
    );
}

#[test]
fn test_error_readonly_assign() {
    expect_error(
        "<?php class User { public readonly $id; public function __construct($id) { $this->id = $id; } } $u = new User(1); $u->id = 2;",
        "Cannot assign to readonly property outside constructor: User::id",
    );
}

#[test]
fn test_error_static_this() {
    expect_error(
        "<?php class Demo { public static function bad() { return $this; } } Demo::bad();",
        "Cannot use $this inside a static method",
    );
}

#[test]
fn test_error_wrong_constructor_args() {
    expect_error(
        "<?php class Point { public function __construct($x) {} } $p = new Point();",
        "Constructor 'Point::__construct' expects 1 arguments, got 0",
    );
}

#[test]
fn test_error_array_literal_rejects_unrelated_object_types() {
    expect_error(
        "<?php class Dog {} class Car {} $items = [new Dog(), new Car()];",
        "Array element type mismatch",
    );
}

#[test]
fn test_error_parent_outside_class_scope() {
    expect_error(
        "<?php parent::boot();",
        "Cannot use parent:: outside class method scope",
    );
}

#[test]
fn test_error_self_outside_class_scope() {
    expect_error(
        "<?php self::boot();",
        "Cannot use self:: outside class method scope",
    );
}

#[test]
fn test_error_static_outside_class_scope() {
    expect_error(
        "<?php static::boot();",
        "Cannot use static:: outside class method scope",
    );
}

#[test]
fn test_error_parent_without_parent_class() {
    expect_error(
        "<?php class Solo { public function boot() { return parent::boot(); } } $s = new Solo(); $s->boot();",
        "Class Solo has no parent class",
    );
}

#[test]
fn test_error_self_instance_method_from_static_method() {
    expect_error(
        "<?php class Box { public static function run() { return self::value(); } public function value() { return 1; } } echo Box::run();",
        "Cannot call self instance method from a static method",
    );
}

#[test]
fn test_error_circular_inheritance() {
    expect_error(
        "<?php class A extends B {} class B extends A {}",
        "Circular inheritance detected",
    );
}

#[test]
fn test_error_cannot_reduce_visibility_when_overriding_method() {
    expect_error(
        "<?php class Base { public function ping() { return 1; } } class Child extends Base { protected function ping() { return 2; } }",
        "Cannot reduce visibility when overriding method: Child::ping",
    );
}

#[test]
fn test_error_subclass_cannot_access_parent_private_property() {
    expect_error(
        "<?php class Base { private $value = 1; } class Child extends Base { public function read() { return $this->value; } } $c = new Child(); echo $c->read();",
        "Cannot access private property: Child::value",
    );
}

#[test]
fn test_error_override_cannot_change_parameter_count() {
    expect_error(
        "<?php class Base { public function ping($x) { return $x; } } class Child extends Base { public function ping() { return 1; } }",
        "Cannot change parameter count when overriding method: Child::ping",
    );
}

#[test]
fn test_error_property_shadowing_across_inheritance_not_supported() {
    expect_error(
        "<?php class Base { public $value = 1; } class Child extends Base { public $value = 2; }",
        "Property redeclaration across inheritance is not yet supported: Child::value",
    );
}

#[test]
fn test_error_missing_interface_method() {
    expect_error(
        "<?php interface Named { public function name(); } class User implements Named {}",
        "Class User must implement interface method Named::name",
    );
}

#[test]
fn test_error_wrong_signature_vs_interface() {
    expect_error(
        "<?php interface Named { public function name($x); } class User implements Named { public function name() { return \"x\"; } }",
        "Cannot change parameter count when implementing interface method: User::name",
    );
}

#[test]
fn test_error_instantiate_abstract_class() {
    expect_error(
        "<?php abstract class Base { abstract public function run(); } $x = new Base();",
        "Cannot instantiate abstract class: Base",
    );
}

#[test]
fn test_error_abstract_method_with_body() {
    expect_error(
        "<?php abstract class Base { abstract public function run() { return 1; } }",
        "Abstract method cannot have a body: Base::run",
    );
}

#[test]
fn test_error_interface_inheritance_cycle() {
    expect_error(
        "<?php interface A extends B {} interface B extends A {}",
        "Circular interface inheritance detected",
    );
}

#[test]
fn test_error_class_cannot_extend_interface() {
    expect_error(
        "<?php interface Named { public function name(); } class User extends Named {}",
        "Class User cannot extend interface Named; use implements instead",
    );
}

// --- Date/time error tests ---

#[test]
fn test_error_date_no_args() {
    expect_error("<?php date();", "date() takes 1 or 2 arguments");
}

#[test]
fn test_error_date_too_many_args() {
    expect_error(r#"<?php date("Y", 0, 0);"#, "date() takes 1 or 2 arguments");
}

#[test]
fn test_error_mktime_wrong_args() {
    expect_error(
        "<?php mktime(1, 2, 3);",
        "mktime() takes exactly 6 arguments",
    );
}

#[test]
fn test_error_strtotime_no_args() {
    expect_error("<?php strtotime();", "strtotime() takes exactly 1 argument");
}

// --- JSON error tests ---

#[test]
fn test_error_json_encode_no_args() {
    expect_error(
        "<?php json_encode();",
        "json_encode() takes exactly 1 argument",
    );
}

#[test]
fn test_error_json_encode_too_many_args() {
    expect_error(
        r#"<?php json_encode("a", "b");"#,
        "json_encode() takes exactly 1 argument",
    );
}

#[test]
fn test_error_json_decode_no_args() {
    expect_error(
        "<?php json_decode();",
        "json_decode() takes exactly 1 argument",
    );
}

#[test]
fn test_error_json_last_error_with_args() {
    expect_error(
        "<?php json_last_error(1);",
        "json_last_error() takes no arguments",
    );
}

// --- Regex error tests ---

#[test]
fn test_error_preg_match_no_args() {
    expect_error(
        "<?php preg_match();",
        "preg_match() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_preg_match_one_arg() {
    expect_error(
        r#"<?php preg_match("/test/");"#,
        "preg_match() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_preg_match_all_no_args() {
    expect_error(
        "<?php preg_match_all();",
        "preg_match_all() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_preg_replace_wrong_args() {
    expect_error(
        r#"<?php preg_replace("/a/", "b");"#,
        "preg_replace() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_preg_split_no_args() {
    expect_error(
        "<?php preg_split();",
        "preg_split() takes exactly 2 arguments",
    );
}

// --- Hex literal errors ---

#[test]
fn test_error_hex_no_digits() {
    expect_error("<?php echo 0x;", "Expected hex digits after '0x'");
}

// --- Mixed return type errors ---

// Note: mixed return types are now widened (Str > Float > Int) instead of
// producing an error. The test_return_type_mixed_branches codegen test
// covers the widening behavior.

// --- Math trig/log error tests ---

#[test]
fn test_error_sin_no_args() {
    expect_error("<?php sin();", "sin() takes exactly 1 argument");
}

#[test]
fn test_error_sin_too_many_args() {
    expect_error("<?php sin(1, 2);", "sin() takes exactly 1 argument");
}

#[test]
fn test_error_cos_no_args() {
    expect_error("<?php cos();", "cos() takes exactly 1 argument");
}

#[test]
fn test_error_atan2_one_arg() {
    expect_error("<?php atan2(1);", "atan2() takes exactly 2 arguments");
}

#[test]
fn test_error_atan2_three_args() {
    expect_error("<?php atan2(1, 2, 3);", "atan2() takes exactly 2 arguments");
}

#[test]
fn test_error_log_no_args() {
    expect_error("<?php log();", "log() takes 1 or 2 arguments");
}

#[test]
fn test_error_log_too_many_args() {
    expect_error("<?php log(1, 2, 3);", "log() takes 1 or 2 arguments");
}

#[test]
fn test_error_hypot_one_arg() {
    expect_error("<?php hypot(1);", "hypot() takes exactly 2 arguments");
}

#[test]
fn test_error_exp_no_args() {
    expect_error("<?php exp();", "exp() takes exactly 1 argument");
}

#[test]
fn test_error_pi_with_arg() {
    expect_error("<?php pi(1);", "pi() takes no arguments");
}

#[test]
fn test_error_deg2rad_no_args() {
    expect_error("<?php deg2rad();", "deg2rad() takes exactly 1 argument");
}

#[test]
fn test_error_closure_use_undefined_variable() {
    expect_error(
        r#"<?php
$fn = function() use ($undefined) { echo $undefined; };
"#,
        "Undefined variable in use(): $undefined",
    );
}

// --- Pointer error tests ---

#[test]
fn test_error_ptr_no_args() {
    expect_error("<?php ptr();", "ptr() takes exactly 1 argument");
}

#[test]
fn test_error_ptr_requires_variable_argument() {
    expect_error("<?php ptr(1 + 2);", "ptr() argument must be a variable");
}

#[test]
fn test_error_ptr_null_with_args() {
    expect_error("<?php ptr_null(1);", "ptr_null() takes 0 arguments");
}

#[test]
fn test_error_ptr_is_null_wrong_args() {
    expect_error(
        "<?php ptr_is_null();",
        "ptr_is_null() takes exactly 1 argument",
    );
}

#[test]
fn test_error_is_null_wrong_args() {
    expect_error("<?php is_null();", "is_null() takes exactly 1 argument");
}

#[test]
fn test_error_ptr_is_null_requires_pointer() {
    expect_error(
        "<?php ptr_is_null(123);",
        "ptr_is_null() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_offset_wrong_args() {
    expect_error(
        "<?php $p = ptr_null(); ptr_offset($p);",
        "ptr_offset() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_ptr_offset_requires_pointer() {
    expect_error(
        "<?php ptr_offset(123, 8);",
        "ptr_offset() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_offset_requires_integer_offset() {
    expect_error(
        "<?php $p = ptr_null(); ptr_offset($p, \"8\");",
        "ptr_offset() second argument must be integer",
    );
}

#[test]
fn test_error_ptr_get_wrong_args() {
    expect_error("<?php ptr_get();", "ptr_get() takes exactly 1 argument");
}

#[test]
fn test_error_ptr_get_requires_pointer() {
    expect_error(
        "<?php ptr_get(123);",
        "ptr_get() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_read8_requires_pointer() {
    expect_error(
        "<?php ptr_read8(123);",
        "ptr_read8() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_read32_requires_pointer() {
    expect_error(
        "<?php ptr_read32(123);",
        "ptr_read32() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_set_wrong_args() {
    expect_error(
        "<?php ptr_set(ptr_null());",
        "ptr_set() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_ptr_set_requires_pointer() {
    expect_error(
        "<?php ptr_set(123, 1);",
        "ptr_set() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_set_requires_word_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_set($p, \"hello\");",
        "ptr_set() value must be int, bool, null, or pointer",
    );
}

#[test]
fn test_error_ptr_write8_requires_int_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write8($p, \"hello\");",
        "ptr_write8() value must be int",
    );
}

#[test]
fn test_error_ptr_write32_requires_int_value() {
    expect_error(
        "<?php $p = ptr_null(); ptr_write32($p, \"hello\");",
        "ptr_write32() value must be int",
    );
}

#[test]
fn test_error_ptr_sizeof_wrong_args() {
    expect_error(
        "<?php ptr_sizeof();",
        "ptr_sizeof() takes exactly 1 argument",
    );
}

#[test]
fn test_error_ptr_sizeof_requires_literal() {
    expect_error(
        "<?php $t = \"int\"; ptr_sizeof($t);",
        "ptr_sizeof() argument must be a string literal",
    );
}

#[test]
fn test_error_ptr_sizeof_unknown_type() {
    expect_error(
        "<?php ptr_sizeof(\"NoSuchType\");",
        "Unknown type for ptr_sizeof(): NoSuchType",
    );
}

#[test]
fn test_error_ptr_cast_missing_type() {
    expect_error(
        "<?php ptr_cast<>(ptr_null());",
        "Expected type name after 'ptr_cast<'",
    );
}

#[test]
fn test_error_ptr_cast_requires_pointer_argument() {
    expect_error(
        "<?php ptr_cast<int>(123);",
        "ptr_cast() requires a pointer argument",
    );
}

#[test]
fn test_error_ptr_cast_rejects_unknown_target() {
    expect_error(
        "<?php $p = ptr_null(); ptr_cast<NoSuchType>($p);",
        "Unknown ptr_cast target type: NoSuchType",
    );
}

#[test]
fn test_error_pointer_loose_comparison_is_rejected() {
    expect_error(
        "<?php $x = 1; $p = ptr($x); $q = ptr($x); echo $p == $q;",
        "Loose pointer comparison is not supported; use === or !==",
    );
}

// --- FFI error tests ---

#[test]
fn test_error_extern_unknown_type() {
    expect_error(
        "<?php extern function foo(badtype $x): int;",
        "Unknown C type: badtype",
    );
}

#[test]
fn test_error_extern_missing_function() {
    expect_error(
        "<?php extern badkw;",
        "Expected 'function', string literal, 'class', or 'global' after 'extern'",
    );
}

#[test]
fn test_error_extern_block_empty() {
    expect_error("<?php extern \"lib\" { }", "Empty extern block");
}

#[test]
fn test_error_extern_wrong_arg_count() {
    expect_error(
        "<?php extern function abs(int $n): int; abs();",
        "Extern function 'abs' expects 1 arguments, got 0",
    );
}

#[test]
fn test_error_extern_wrong_arg_type() {
    expect_error(
        "<?php extern function strlen(string $s): int; strlen(123);",
        "Extern function 'strlen' parameter $s expects Str, got Int",
    );
}

#[test]
fn test_error_duplicate_extern_function() {
    expect_error(
        "<?php extern function foo(int $x): int; extern function foo(int $y): int;",
        "Duplicate function declaration: foo",
    );
}

#[test]
fn test_error_extern_global_reserved_name() {
    expect_error(
        "<?php extern global int $argc;",
        "extern global $argc would shadow a reserved superglobal",
    );
}

#[test]
fn test_error_extern_global_void_type() {
    expect_error(
        "<?php extern global void $bad;",
        "Extern global $bad uses an unsupported type",
    );
}

#[test]
fn test_error_extern_callable_requires_literal_function_name() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; function on_signal($sig) {} $fn = \"on_signal\"; signal(15, $fn);",
        "expects a string literal naming a user function",
    );
}

#[test]
fn test_error_extern_callable_requires_defined_function() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; signal(15, \"missing_handler\");",
        "Undefined callback function: missing_handler",
    );
}

#[test]
fn test_error_extern_callable_requires_c_compatible_return_type() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; function bad_handler($sig) { return \"oops\"; } signal(15, \"bad_handler\");",
        "unsupported return type",
    );
}

#[test]
fn test_error_extern_class_void_field() {
    expect_error(
        "<?php extern class Bad { void $field; }",
        "Extern class 'Bad' field $field uses an unsupported type",
    );
}

#[test]
fn test_error_readonly_class_property_is_implicitly_readonly() {
    expect_error(
        "<?php readonly class User { public $id; public function __construct($id) { $this->id = $id; } } $u = new User(1); $u->id = 2;",
        "Cannot assign to readonly property outside constructor: User::id",
    );
}

#[test]
fn test_error_readonly_class_cannot_extend_non_readonly_parent() {
    expect_error(
        "<?php class Base {} readonly class Child extends Base {}",
        "readonly class cannot extend non-readonly parent",
    );
}

#[test]
fn test_error_first_class_callable_rejects_instance_methods() {
    expect_error(
        "<?php class User { public function greet() { return 1; } } $u = new User(); $f = $u->greet(...);",
        "First-class instance method callables are not supported yet",
    );
}

#[test]
fn test_error_first_class_callable_rejects_static_receiver_static() {
    expect_error(
        "<?php class User { public static function make() { return 1; } public function run() { $f = static::make(...); } }",
        "does not support static:: targets yet",
    );
}

#[test]
fn test_error_first_class_callable_rejects_unsupported_builtin() {
    expect_error(
        "<?php $f = trim(...);",
        "does not support builtin 'trim' yet",
    );
}

#[test]
fn test_error_first_class_callable_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); $f(1);",
        "parameter $n must be passed a variable",
    );
}

#[test]
fn test_error_closure_ref_param_requires_variable() {
    expect_error(
        "<?php $f = function (&$x) { $x = $x + 1; }; $f(1);",
        "parameter $x must be passed a variable",
    );
}

#[test]
fn test_error_call_user_func_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); call_user_func($f, 1);",
        "parameter $n must be passed a variable",
    );
}

#[test]
fn test_error_call_user_func_string_literal_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } call_user_func(\"bump\", 1);",
        "parameter $n must be passed a variable",
    );
}

#[test]
fn test_error_call_user_func_array_rejects_ref_callback_params() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); $value = 1; call_user_func_array($f, [$value]);",
        "does not support pass-by-reference callback parameters yet",
    );
}

#[test]
fn test_error_call_user_func_array_string_literal_rejects_ref_callback_params() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $value = 1; call_user_func_array(\"bump\", [$value]);",
        "does not support pass-by-reference callback parameters yet",
    );
}
