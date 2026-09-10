//! Purpose:
//! Exercises public mbregex settings through AOT, opaque eval, and callable dispatch.
//!
//! Called from:
//! - The focused codegen string integration suite.
//!
//! Key details:
//! - PHP 8.5.10 returns previous options and accepts C-string encoding aliases.
//! - Stringable callbacks can change settings before the outer setter commits.

use crate::support::*;

/// Keeps eval source unknown during compilation while preserving the original PHP bytes.
fn program(body: &str, eval: bool) -> String {
    if !eval { return format!("<?php {body}"); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Covers canonical getters, named calls, aliases, source ordering, and text-setting independence.
#[test]
fn test_mbstring_regex_settings_public_calls() {
    let body = r#"
namespace RegexSettings;
echo Mb_ReGeX_EnCoDiNg(), ":", mb_regex_set_options(null), "\n";
echo mb_regex_encoding(encoding: "UTF-16LE") ? "set\n" : "wrong\n";
mb_internal_encoding("ASCII");
echo mb_regex_encoding(null), ":", mb_internal_encoding(), "\n";
$encoding = mb_regex_encoding(...);
echo $encoding("SJIS-win") ? "alias\n" : "wrong\n";
$dynamic = $argc > 0 ? "mb_regex_encoding" : "mb_internal_encoding";
echo $dynamic(), ":", call_user_func_array($dynamic, ["encoding" => null]), "\n";
echo mb_regex_encoding("UTF8\0ignored") ? "nul alias\n" : "wrong\n";
$options = mb_regex_set_options(...);
echo $options(options: "ixmslnj"), ":", $options(), "\n";
echo call_user_func_array("mb_regex_set_options", ["options" => ""]), ":", mb_regex_set_options(), "\n";
class RegexOptions {
    public function __toString(): string { mb_regex_set_options("im"); return "z"; }
}
echo mb_regex_set_options(new RegexOptions()), ":", mb_regex_set_options(), "\n";
echo mb_regex_set_options(false), ":", mb_regex_set_options(), "\n";
"#;
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, "UTF-8:pr\nset\nUTF-16LE:ASCII\nalias\nSJIS:SJIS\nnul alias\npr:ixplnj\nixplnj:r\nimr:z\nz:r\n", "eval={eval}");
        assert!(output.stderr.is_empty(), "eval={eval}: {}", output.stderr);
    }
}

/// Keeps value and dynamic arity/type errors catchable without replacing the last valid setting.
#[test]
fn test_mbstring_regex_settings_runtime_errors() {
    let body = r#"
$encoding = $argc > 0 ? "mb_regex_encoding" : "mb_internal_encoding";
$options = $argc > 0 ? "mb_regex_set_options" : "mb_language";
mb_regex_encoding("ASCII");
mb_regex_set_options("ixm");
try { $encoding(null, null); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { $options(null, null); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { $encoding([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $options([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { mb_regex_encoding("BASE64"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_regex_set_options("jbq"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_regex_set_options("i\0m"); } catch (ValueError $e) { echo bin2hex($e->getMessage()), "\n"; }
try { mb_regex_set_options(chr(255)); } catch (ValueError $e) { echo bin2hex($e->getMessage()), "\n"; }
echo mb_regex_encoding(), ":", mb_regex_set_options(), "\n";
"#;
    let expected = concat!(
        "mb_regex_encoding() expects at most 1 argument, 2 given\n",
        "mb_regex_set_options() expects at most 1 argument, 2 given\n",
        "mb_regex_encoding(): Argument #1 ($encoding) must be of type ?string, array given\n",
        "mb_regex_set_options(): Argument #1 ($options) must be of type ?string, array given\n",
        "mb_regex_encoding(): Argument #1 ($encoding) must be a valid encoding, \"BASE64\" given\n",
        "Option \"q\" is not supported\n4f7074696f6e2022\n",
        "4f7074696f6e2022ff22206973206e6f7420737570706f72746564\nASCII:ixmr\n",
    );
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "eval={eval}");
        assert!(output.stderr.is_empty(), "eval={eval}: {}", output.stderr);
    }
}

/// Observes the same session before, during, and after a runtime-unknown eval invocation.
#[test]
fn test_mbstring_regex_settings_share_aot_eval_state() {
    let source = r#"<?php
mb_regex_encoding("ASCII");
mb_regex_set_options("j");
$source = $argc > 0 ? 'echo mb_regex_encoding(), ":", mb_regex_set_options(), "\n"; mb_regex_encoding("UTF-16LE"); mb_regex_set_options("ip");' : '';
eval($source);
echo mb_regex_encoding(), ":", mb_regex_set_options(), "\n";
"#;
    let output = compile_and_run_capture(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "ASCII:j\nUTF-16LE:ipr\n");
    assert!(output.stderr.is_empty(), "{}", output.stderr);
}
