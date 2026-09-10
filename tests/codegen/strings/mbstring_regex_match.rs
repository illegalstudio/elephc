//! Purpose:
//! Verifies managed Oniguruma matching through the shared AOT and opaque eval boundary.
//!
//! Called from:
//! - Focused mbstring codegen tests with the reviewed native package fixture.
//!
//! Key details:
//! - Matching honors live regex settings, Stringable side effects, and exact diagnostics.
//! - Opaque eval requires explicit activation when no reachable matching call exists.

use crate::support::*;

/// Keeps eval source opaque while an explicit native call activates the shared provider.
fn program(body: &str, eval: bool) -> String {
    if !eval { return format!("<?php {body}"); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php mb_ereg_match('', ''); $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Preserves anchored matches, PHP parameter names, callable dispatch, and multibyte subject validation.
#[test]
fn test_mbstring_regex_match_public_calls() {
    let body = r#"
namespace RegexMatch;
echo (Mb_ErEg_MaTcH('ab', 'abc') ? "y" : "n"), ":";
echo (mb_ereg_match('bc', 'abc') ? "y" : "n"), ":";
echo (mb_ereg_match('^[A-Z][A-Za-z0-9]*$', 'Foo') ? "y" : "n"), ":";
echo (mb_ereg_match('[a-z]+\z', 'abc123') ? "y" : "n"), ":";
echo (mb_ereg_match('ab', 'AB') ? "y" : "n"), ":";
echo (mb_ereg_match('ab', 'AB', 'i') ? "y" : "n"), ":";
echo (mb_ereg_match('ab', 'AB', null) ? "y" : "n"), ":";
echo (call_user_func("mb_ereg_match", "ab", "abx") ? "y" : "n"), ":";
echo function_exists("mb_ereg_match") ? "available\n" : "missing\n";
$match = mb_ereg_match(...);
echo $match(string: "猫", pattern: ".") ? "unicode\n" : "wrong\n";
echo call_user_func_array("mb_ereg_match", ["string" => "ABC", "pattern" => "abc", "options" => "i"]) ? "named\n" : "wrong\n";
mb_regex_set_options("i");
var_dump(mb_ereg_match("abc", "ABC"), mb_ereg_match("abc", "ABC", null), mb_ereg_match("abc", "ABC", ""));
mb_internal_encoding("ASCII");
var_dump(mb_ereg_match(".", "猫"), mb_ereg_match("", ""), mb_ereg_match("a\0b", "a\0bc"));
mb_regex_encoding("UTF-16LE");
var_dump(mb_ereg_match(".\0", "猫\0"), mb_ereg_match(".\0", "a\0"), mb_ereg_match(".\0", "a"));
mb_regex_encoding("SJIS-WIN");
var_dump(mb_ereg_match(".", chr(250) . chr(64)));
mb_regex_encoding("SJIS");
var_dump(mb_ereg_match(".", chr(250) . chr(64)));
"#;
    let expected = concat!("y:n:y:n:n:y:n:y:available\nunicode\nnamed\n",
        "bool(true)\nbool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\n",
        "bool(true)\nbool(true)\nbool(false)\nbool(true)\nbool(false)\n");
    for eval in [false, true] { assert_eq!(compile_and_run(&program(body, eval)), expected, "eval={eval}"); }
}

/// Applies Stringable side effects before matching and preserves catchable argument and option errors.
#[test]
fn test_mbstring_regex_match_runtime_errors() {
    let body = r#"
class RegexPattern {
    public function __toString(): string { mb_regex_set_options("i"); return "abc"; }
}
var_dump(mb_ereg_match(new RegexPattern(), "ABC"));
$match = $argc > 0 ? "mb_ereg_match" : "mb_check_encoding";
try { $match(); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { $match("a", []); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump(mb_ereg_match("[", "a"));
try { mb_ereg_match("a", "a", "q"); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
echo mb_regex_set_options(), ":", mb_regex_encoding(), "\n";
var_dump(mb_ereg_match("a", "abc"));
"#;
    let expected = concat!("bool(true)\nmb_ereg_match() expects at least 2 arguments, 0 given\n",
        "mb_ereg_match(): Argument #2 ($string) must be of type string, array given\n",
        "bool(false)\nOption \"q\" is not supported\nir:UTF-8\nbool(true)\n");
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "eval={eval}");
        assert_eq!(output.stderr, "Warning: mb_ereg_match(): mbregex compile err: premature end of char-class\n", "eval={eval}");
    }
}

/// Enables opaque eval matching and progressive searches with mbstring while keeping PCRE2 independent.
#[test]
fn test_mbstring_regex_match_forced_eval_capability() {
    for enabled in [false, true] {
        let dir = make_cli_test_dir("elephc_mbregex_eval_capability");
        let php = dir.join("main.php");
        fs::write(&php, "<?php $source = (string)getenv('ELEPHC_MBREGEX_SOURCE'); eval($source);").unwrap();
        let mut command = if enabled { elephc_cli_command_with_oniguruma(&dir) } else { elephc_cli_command(&dir) };
        if enabled { command.arg("--with-mbstring"); }
        let compiled = command.arg(&php).output().unwrap();
        assert!(compiled.status.success(), "enabled={enabled}: {}", String::from_utf8_lossy(&compiled.stderr));
        let executed = Command::new(php.with_extension("")).env("ELEPHC_MBREGEX_SOURCE", r#"
echo function_exists('mb_ereg_match') ? 'onig' : 'none';
echo function_exists('preg_match') ? ':pcre' : ':none';
if (function_exists('mb_ereg_match')) { echo mb_ereg_match('a', 'abc') ? ':match' : ':wrong'; }
foreach (['mb_ereg_search_init', 'mb_ereg_search', 'mb_ereg_search_pos',
          'mb_ereg_search_regs', 'mb_ereg_search_getpos', 'mb_ereg_search_getregs',
          'mb_ereg_search_setpos', 'mb_split', 'mb_ereg_replace', 'mb_eregi_replace'] as $name) {
    echo function_exists($name) ? ':on' : ':off';
}
if (function_exists('mb_ereg_search_init')) {
    mb_ereg_search_init('abc', 'b');
    echo mb_ereg_search() ? ':search' : ':wrong';
    echo ':', mb_ereg_search_getpos();
    echo mb_ereg_search_setpos(0) ? ':reset' : ':wrong';
    $position = mb_ereg_search_pos();
    echo ':', $position[0], ':', $position[1];
    mb_ereg_search_setpos(0);
    $registers = mb_ereg_search_regs();
    $retained = mb_ereg_search_getregs();
    echo ':', $registers[0], ':', $retained[0];
    $fields = mb_split(',', 'a,b');
    echo ':', $fields[0], ':', $fields[1];
    echo ':', mb_ereg_replace('a', 'X', 'aba'), ':', mb_eregi_replace('a', 'Y', 'aAb');
}
"#)
            .output().unwrap();
        assert!(executed.status.success(), "enabled={enabled}: {}", String::from_utf8_lossy(&executed.stderr));
        assert_eq!(executed.stdout, if enabled {
            b"onig:none:match:on:on:on:on:on:on:on:on:on:on:search:2:reset:1:1:b:b:a:b:XbX:YYb".as_slice()
        } else {
            b"none:none:off:off:off:off:off:off:off:off:off:off".as_slice()
        });
        assert!(executed.stderr.is_empty(), "{}", String::from_utf8_lossy(&executed.stderr));
        fs::remove_dir_all(dir).unwrap();
    }
}
