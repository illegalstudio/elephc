use elephc::lexer::tokenize;
use elephc::parser::parse;
use elephc::types;

fn check_source(src: &str) -> Result<(), String> {
    let tokens = tokenize(src).map_err(|e| e.message.clone())?;
    let ast = parse(&tokens).map_err(|e| e.message.clone())?;
    types::check(&ast).map_err(|e| e.message.clone())?;
    Ok(())
}

fn expect_error(src: &str, expected_substr: &str) {
    match check_source(src) {
        Ok(_) => panic!("Expected error containing '{}', but got Ok", expected_substr),
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
    expect_error(
        "<?php $x = 42; $x = \"hello\";",
        "cannot reassign $x",
    );
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
        "expects 1 to 1 arguments, got 2",
    );
}

#[test]
fn test_error_increment_string() {
    expect_error(
        "<?php $x = \"hi\"; $x++;",
        "Cannot increment/decrement",
    );
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
    expect_error("<?php round();", "round() takes exactly 1 argument");
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
    expect_error("<?php min(1);", "min() takes exactly 2 arguments");
}

#[test]
fn test_error_max_wrong_args() {
    expect_error("<?php max(1);", "max() takes exactly 2 arguments");
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
    expect_error("<?php is_infinite();", "is_infinite() takes exactly 1 argument");
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
    expect_error("<?php random_int(1);", "random_int() takes exactly 2 arguments");
}

#[test]
fn test_error_number_format_wrong_args() {
    expect_error("<?php number_format();", "number_format() takes 1 to 4 arguments");
}

// --- String function errors ---

#[test]
fn test_error_substr_wrong_args() {
    expect_error("<?php substr(\"hi\");", "substr() takes 2 or 3 arguments");
}

#[test]
fn test_error_strpos_wrong_args() {
    expect_error("<?php strpos(\"hi\");", "strpos() takes exactly 2 arguments");
}

#[test]
fn test_error_str_replace_wrong_args() {
    expect_error("<?php str_replace(\"a\", \"b\");", "str_replace() takes exactly 3 arguments");
}

#[test]
fn test_error_sprintf_no_args() {
    expect_error("<?php sprintf();", "sprintf() requires at least 1 argument");
}

#[test]
fn test_error_explode_wrong_args() {
    expect_error("<?php explode(\",\");", "explode() takes exactly 2 arguments");
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
    expect_error("<?php htmlspecialchars();", "htmlspecialchars() takes exactly 1 argument");
}

#[test]
fn test_error_urlencode_wrong_args() {
    expect_error("<?php urlencode();", "urlencode() takes exactly 1 argument");
}

#[test]
fn test_error_base64_encode_wrong_args() {
    expect_error("<?php base64_encode();", "base64_encode() takes exactly 1 argument");
}

#[test]
fn test_error_ctype_alpha_wrong_args() {
    expect_error("<?php ctype_alpha();", "ctype_alpha() takes exactly 1 argument");
}

#[test]
fn test_error_hash_wrong_args() {
    expect_error(r#"<?php hash("md5");"#, "hash() takes exactly 2 arguments");
}

#[test]
fn test_error_sscanf_wrong_args() {
    expect_error(r#"<?php sscanf("hi");"#, "sscanf() takes at least 2 arguments");
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
    expect_error(r#"<?php fopen("file");"#, "fopen() takes exactly 2 arguments");
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
    expect_error("<?php file_get_contents();", "file_get_contents() takes exactly 1 argument");
}

#[test]
fn test_error_file_put_contents_wrong_args() {
    expect_error(r#"<?php file_put_contents("x");"#, "file_put_contents() takes exactly 2 arguments");
}

#[test]
fn test_error_file_exists_wrong_args() {
    expect_error("<?php file_exists();", "file_exists() takes exactly 1 argument");
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
    expect_error(r#"<?php rename("x");"#, "rename() takes exactly 2 arguments");
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
    expect_error(r#"<?php tempnam("x");"#, "tempnam() takes exactly 2 arguments");
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
    expect_error("<?php is_readable();", "is_readable() takes exactly 1 argument");
}

#[test]
fn test_error_is_writable_wrong_args() {
    expect_error("<?php is_writable();", "is_writable() takes exactly 1 argument");
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
    expect_error("<?php sys_get_temp_dir(1);", "sys_get_temp_dir() takes no arguments");
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
    expect_error(r#"<?php readline(1, 2);"#, "readline() takes 0 or 1 arguments");
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
fn test_error_assoc_array_mixed() {
    expect_error("<?php $a = [\"b\" => 2, 1];", "Cannot mix");
}

// --- v0.6: array function argument errors ---

#[test]
fn test_error_array_reverse_wrong_args() {
    expect_error("<?php array_reverse();", "array_reverse() takes exactly 1 argument");
}

#[test]
fn test_error_array_merge_wrong_args() {
    expect_error("<?php $a = [1]; array_merge($a);", "array_merge() takes exactly 2 arguments");
}

#[test]
fn test_error_array_sum_wrong_args() {
    expect_error("<?php array_sum();", "array_sum() takes exactly 1 argument");
}

#[test]
fn test_error_array_search_wrong_args() {
    expect_error("<?php $a = [1]; array_search($a);", "array_search() takes exactly 2 arguments");
}

#[test]
fn test_error_array_key_exists_wrong_args() {
    expect_error("<?php array_key_exists(1);", "array_key_exists() takes exactly 2 arguments");
}

#[test]
fn test_error_array_slice_wrong_args() {
    expect_error("<?php $a = [1]; array_slice($a);", "array_slice() takes 2 or 3 arguments");
}

#[test]
fn test_error_array_combine_wrong_args() {
    expect_error("<?php $a = [1]; array_combine($a);", "array_combine() takes exactly 2 arguments");
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
    expect_error("<?php array_fill(0, 5);", "array_fill() takes exactly 3 arguments");
}

#[test]
fn test_error_array_push_wrong_args() {
    expect_error("<?php array_push();", "array_push() takes exactly 2 arguments");
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
    expect_error("<?php array_keys();", "array_keys() takes exactly 1 argument");
}

#[test]
fn test_error_array_values_wrong_args() {
    expect_error("<?php array_values();", "array_values() takes exactly 1 argument");
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
    expect_error("<?php array_unique();", "array_unique() takes exactly 1 argument");
}

#[test]
fn test_error_array_product_wrong_args() {
    expect_error("<?php array_product();", "array_product() takes exactly 1 argument");
}

#[test]
fn test_error_array_shift_wrong_args() {
    expect_error("<?php array_shift();", "array_shift() takes exactly 1 argument");
}

#[test]
fn test_error_array_unshift_wrong_args() {
    expect_error("<?php array_unshift();", "array_unshift() takes exactly 2 arguments");
}

#[test]
fn test_error_array_splice_wrong_args() {
    expect_error("<?php array_splice();", "array_splice() takes 2 or 3 arguments");
}

#[test]
fn test_error_array_flip_wrong_args() {
    expect_error("<?php array_flip();", "array_flip() takes exactly 1 argument");
}

#[test]
fn test_error_array_chunk_wrong_args() {
    expect_error("<?php array_chunk();", "array_chunk() takes exactly 2 arguments");
}

#[test]
fn test_error_array_pad_wrong_args() {
    expect_error("<?php array_pad();", "array_pad() takes exactly 3 arguments");
}

#[test]
fn test_error_array_fill_keys_wrong_args() {
    expect_error("<?php array_fill_keys();", "array_fill_keys() takes exactly 2 arguments");
}

#[test]
fn test_error_array_diff_wrong_args() {
    expect_error("<?php array_diff();", "array_diff() takes exactly 2 arguments");
}

#[test]
fn test_error_array_intersect_wrong_args() {
    expect_error("<?php array_intersect();", "array_intersect() takes exactly 2 arguments");
}

#[test]
fn test_error_array_diff_key_wrong_args() {
    expect_error("<?php array_diff_key();", "array_diff_key() takes exactly 2 arguments");
}

#[test]
fn test_error_array_intersect_key_wrong_args() {
    expect_error("<?php array_intersect_key();", "array_intersect_key() takes exactly 2 arguments");
}

#[test]
fn test_error_array_rand_wrong_args() {
    expect_error("<?php array_rand();", "array_rand() takes exactly 1 argument");
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
    expect_error("<?php natcasesort();", "natcasesort() takes exactly 1 argument");
}

#[test]
fn test_error_array_column_wrong_args() {
    expect_error(r#"<?php array_column([]);"#, "array_column() takes exactly 2 arguments");
}

#[test]
fn test_error_array_map_wrong_args() {
    expect_error(r#"<?php array_map("fn");"#, "array_map() takes exactly 2 arguments");
}

#[test]
fn test_error_array_filter_wrong_args() {
    expect_error(r#"<?php array_filter([]);"#, "array_filter() takes exactly 2 arguments");
}

#[test]
fn test_error_array_reduce_wrong_args() {
    expect_error(r#"<?php array_reduce([], "fn");"#, "array_reduce() takes exactly 3 arguments");
}

#[test]
fn test_error_array_walk_wrong_args() {
    expect_error(r#"<?php array_walk([]);"#, "array_walk() takes exactly 2 arguments");
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
    expect_error(r#"<?php call_user_func();"#, "call_user_func() takes at least 1 argument");
}

#[test]
fn test_error_function_exists_wrong_args() {
    expect_error(r#"<?php function_exists();"#, "function_exists() takes exactly 1 argument");
}

// --- Closure / arrow function errors ---

#[test]
fn test_error_call_non_callable_variable() {
    expect_error(
        r#"<?php $x = 5; $x(1);"#,
        "not a callable",
    );
}

#[test]
fn test_error_arrow_function_missing_arrow() {
    expect_error(
        r#"<?php $f = fn($x) $x * 2;"#,
        "Expected '=>'",
    );
}

#[test]
fn test_error_arrow_function_missing_lparen() {
    expect_error(
        r#"<?php $f = fn $x => $x * 2;"#,
        "Expected '(' after 'fn'",
    );
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
    expect_error(
        "<?php echo <<<EOT\nHello",
        "Unterminated heredoc",
    );
}

// --- Constants errors ---

#[test]
fn test_error_undefined_constant() {
    expect_error(
        "<?php echo UNDEFINED_CONST;",
        "Undefined constant",
    );
}

#[test]
fn test_error_const_missing_name() {
    expect_error(
        "<?php const = 5;",
        "Expected constant name",
    );
}

#[test]
fn test_error_const_missing_value() {
    expect_error(
        "<?php const MAX;",
        "Expected '='",
    );
}

#[test]
fn test_error_define_wrong_args() {
    expect_error(
        "<?php define(\"X\");",
        "define() takes exactly 2 arguments",
    );
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
    expect_error(
        "<?php [$a, $b] = 42;",
        "List unpacking requires an array",
    );
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
    expect_error("<?php microtime(1, 2);", "microtime() takes 0 or 1 arguments");
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
    expect_error("<?php php_uname(1, 2);", "php_uname() takes 0 or 1 arguments");
}

#[test]
fn test_error_exec_wrong_args() {
    expect_error("<?php exec();", "exec() takes exactly 1 argument");
}

#[test]
fn test_error_shell_exec_wrong_args() {
    expect_error("<?php shell_exec();", "shell_exec() takes exactly 1 argument");
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
    expect_error("<?php function foo(... ) {}", "Expected variable after '...'");
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

// --- Date/time error tests ---

#[test]
fn test_error_date_no_args() {
    expect_error("<?php date();", "date() takes 1 or 2 arguments");
}

#[test]
fn test_error_date_too_many_args() {
    expect_error(
        r#"<?php date("Y", 0, 0);"#,
        "date() takes 1 or 2 arguments",
    );
}

#[test]
fn test_error_mktime_wrong_args() {
    expect_error("<?php mktime(1, 2, 3);", "mktime() takes exactly 6 arguments");
}

#[test]
fn test_error_strtotime_no_args() {
    expect_error("<?php strtotime();", "strtotime() takes exactly 1 argument");
}

// --- JSON error tests ---

#[test]
fn test_error_json_encode_no_args() {
    expect_error("<?php json_encode();", "json_encode() takes exactly 1 argument");
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
    expect_error("<?php json_decode();", "json_decode() takes exactly 1 argument");
}

#[test]
fn test_error_json_last_error_with_args() {
    expect_error("<?php json_last_error(1);", "json_last_error() takes no arguments");
}

// --- Regex error tests ---

#[test]
fn test_error_preg_match_no_args() {
    expect_error("<?php preg_match();", "preg_match() takes exactly 2 arguments");
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
    expect_error("<?php preg_match_all();", "preg_match_all() takes exactly 2 arguments");
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
    expect_error("<?php preg_split();", "preg_split() takes exactly 2 arguments");
}

// --- Hex literal errors ---

#[test]
fn test_error_hex_no_digits() {
    expect_error("<?php echo 0x;", "Expected hex digits after '0x'");
}

// --- Mixed return type errors ---

#[test]
fn test_error_mixed_return_types() {
    expect_error(
        r#"<?php
function test($x) { if ($x > 0) { return "positive"; } return 0; }
echo test(1);
"#,
        "mixed return types",
    );
}
