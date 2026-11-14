//! Purpose:
//! Hand-written end-to-end fixtures for the `ext/tidy` bridge: repair, document and
//! node objects, configuration errors, casts, and the output handler.
//!
//! Called from:
//! - `cargo test --test codegen_tests tidy` through Rust's test harness.
//!
//! Key details:
//! - Every fixture links `elephc_tidy` and the managed native `libtidy` 5.8.0 archive
//!   from this machine's cache. The corpus in `tidy_php_src.rs` carries the full
//!   php-src surface; these are the focused regression checks.
//! - Each test starts with `skip_without_tidy_native(...)`, so a developer machine
//!   without the from-source package skips visibly and CI turns that into a panic.

use crate::support::*;

/// Clears compile-time INI overrides when a fixture finishes, even on panic.
struct IniRestore;

impl Drop for IniRestore {
    /// Restores the no-override default.
    fn drop(&mut self) {
        elephc::codegen::set_ini_overrides(Vec::new());
    }
}

/// `tidy_repair_string()` repairs a body-only fragment exactly like PHP.
#[test]
fn test_tidy_repair_string_body_only() {
    if skip_without_tidy_native("test_tidy_repair_string_body_only") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$out = tidy_repair_string('<B>testing</I>', ['show-body-only' => true]);
var_dump($out);
"#,
    );
    assert_eq!(out.trim_end(), "string(14) \"<b>testing</b>\"");
}

/// The document and node objects expose names, attributes, text, casts and errors.
#[test]
fn test_tidy_document_and_node_surface() {
    if skip_without_tidy_native("test_tidy_document_and_node_surface") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$t = tidy_parse_string('<HTML><BODY BGCOLOR=#FFFFFF><B>Hi</B></BODY></HTML>', ['newline' => 'LF']);
var_dump(extension_loaded('tidy'));
var_dump(phpversion('tidy') === phpversion());
$body = $t->body();
echo $body->name, ' ', $body->attribute['bgcolor'], "\n";
echo (string) $body->child[0]->child[0], "\n";
var_dump((int) $body, (float) $body, (bool) $body);
try { tidy_getopt($t, 'bogus'); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { tidy_get_output(1); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$u = new tidy;
try { $u->getHtmlVer(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out.trim_end(),
        "bool(true)\nbool(true)\nbody #FFFFFF\nHi\nint(0)\nfloat(0)\nbool(true)\n\
         tidy_getopt(): Argument #2 ($option) is an invalid configuration option, \"bogus\" given\n\
         tidy_get_output(): Argument #1 ($tidy) must be of type tidy, int given\n\
         tidy object is not initialized"
    );
}

/// Invalid configuration arrays produce PHP's ValueError/TypeError split.
#[test]
fn test_tidy_configuration_errors() {
    if skip_without_tidy_native("test_tidy_configuration_errors") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
try {
    tidy_parse_string('<p>x</p>', ['bogus' => 1]);
} catch (ValueError $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}
try {
    tidy_parse_string('<p>x</p>', []);
} catch (TypeError $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}
try {
    tidy_parse_string('<p>x</p>', ['neither']);
} catch (TypeError $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}
try {
    tidy_parse_string('<p>x</p>', ['doctype-mode' => 'customtag']);
} catch (ValueError $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}
try {
    tidy_parse_string('<p>x</p>', ['doctype' => 'php']);
} catch (TypeError $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}
$config = tidy_get_config(tidy_parse_string('<p>x</p>', ['wrap' => '1x']));
var_dump($config['wrap']);
"#,
    );
    assert_eq!(
        out.trim_end(),
        "ValueError: tidy_parse_string(): Argument #2 ($config) Unknown Tidy configuration option \"bogus\"\n\
         TypeError: tidy_parse_string(): Argument #2 ($config) must be of type array with keys as string\n\
         TypeError: tidy_parse_string(): Argument #2 ($config) must be of type array with keys as string\n\
         ValueError: tidy_parse_string(): Argument #2 ($config) Attempting to set read-only option \"doctype-mode\"\n\
         TypeError: tidy_parse_string(): Argument #2 ($config) option \"doctype\" does not accept \"php\" as a value\n\
         int(1)"
    );
}

/// The tidy surface is available inside eval through the native-function bridge once
/// the prelude is linked. Objects created in eval are real host `tidy`/`tidyNode` objects.
#[test]
fn test_tidy_surface_inside_eval() {
    if skip_without_tidy_native("test_tidy_surface_inside_eval") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
tidy_get_release();
eval('
var_dump(extension_loaded("tidy"), in_array("tidy", get_loaded_extensions()));
var_dump(phpversion("tidy") === phpversion());
var_dump(function_exists("tidy_parse_string"), class_exists("tidy"), class_exists("tidyNode"));
$t = tidy_parse_string("<p>x</p>");
echo get_class($t), " ", tidy_get_status($t), " ", $t->body()->name, "\n";
$u = new tidy;
$u->parseString("<b>y</b>");
echo $u->body()->child[0]->child[0]->value, "\n";
echo tidy_get_output($t);
');
"#,
    );
    assert_eq!(
        out.trim_end(),
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\ntidy 1 body\ny\n<html>\n<head>\n<title></title>\n</head>\n<body>\n<p>x</p>\n</body>\n</html>"
    );
}

/// Repair keeps php-src's "warn but continue" behavior for configuration and encoding
/// failures, while parse treats an unsupported encoding as fatal. An empty encoding is
/// skipped by repair but attempted by parse.
#[test]
fn test_tidy_repair_continues_after_encoding_failure() {
    if skip_without_tidy_native("test_tidy_repair_continues_after_encoding_failure") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
var_dump(@tidy_repair_string('<B>testing</I>', ['show-body-only' => true], 'invalid-encoding-name'));
var_dump(@tidy_parse_string('<b>x</b>', null, 'invalid-encoding-name'));
var_dump(@tidy_parse_string('<b>x</b>', null, ''));
var_dump(@tidy_repair_string('<B>testing</I>', ['show-body-only' => true], ''));
"#,
    );
    assert_eq!(
        out.trim_end(),
        "string(14) \"<b>testing</b>\"\nbool(false)\nbool(false)\nstring(14) \"<b>testing</b>\""
    );
}

/// The constructor's EH_THROW window converts a missing configuration file into php's base
/// `Exception`; a malformed file is an `E_NOTICE`, which is *not* converted, so construction
/// continues. The notice is emitted through `trigger_error()` and therefore carries
/// `E_USER_NOTICE` in elephc, a documented adapter difference.
#[test]
fn test_tidy_constructor_config_failure_matches_php_error_window() {
    if skip_without_tidy_native("test_tidy_constructor_config_failure_matches_php_error_window") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$seen = [];
set_error_handler(function (int $severity, string $message) use (&$seen): bool {
    $seen[] = $severity . ':' . $message;
    return true;
});
try {
    new tidy(__FILE__, '/definitely/missing/tidy.conf');
} catch (Exception $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}
file_put_contents(__DIR__ . '/bad-tidy.conf', "indent:@\n");
$constructed = new tidy(__FILE__, __DIR__ . '/bad-tidy.conf');
restore_error_handler();
echo $constructed::class, "\n";
echo count($seen), ':', str_replace(__DIR__, '', $seen[0] ?? ''), "\n";
"#,
    );
    assert_eq!(
        out.trim_end(),
        "Exception: tidy::__construct(): Could not load the Tidy configuration file \"/definitely/missing/tidy.conf\"\n\
         tidy\n\
         1:1024:tidy::__construct(): There were errors while parsing the Tidy configuration file \"/bad-tidy.conf\""
    );
}

/// A `tidy.default_config` warning raised while `new tidy` runs names the enclosing
/// user function, while a top-level construction names `main()`.
#[test]
fn test_tidy_default_config_warning_uses_the_callers_label() {
    if skip_without_tidy_native("test_tidy_default_config_warning_uses_the_callers_label") {
        return;
    }
    let _restore = IniRestore;
    elephc::codegen::set_ini_overrides(vec![(
        "tidy.default_config".to_string(),
        "/definitely/missing/tidy.conf".to_string(),
    )]);
    let output = compile_and_run_capture_merged(
        r#"<?php
function makeTidy(): tidy { return new tidy; }
$inside = makeTidy();
$top = new tidy;
echo "done\n";
"#,
    );
    let out = format!("{}{}", output.stdout, output.stderr);
    assert!(
        out.contains("makeTidy(): Could not load the Tidy configuration file \"/definitely/missing/tidy.conf\""),
        "missing enclosing-function warning:\n{out}"
    );
    assert!(
        out.contains("main(): Could not load the Tidy configuration file \"/definitely/missing/tidy.conf\""),
        "missing main() warning:\n{out}"
    );
    assert!(out.trim_end().ends_with("done"));
}

/// An open_basedir violation on an input file emits php-src's open_basedir warning and,
/// for parse surfaces, the separate "Cannot load" warning.
#[test]
fn test_tidy_open_basedir_input_file_warnings() {
    if skip_without_tidy_native("test_tidy_open_basedir_input_file_warnings") {
        return;
    }
    let _restore = IniRestore;
    elephc::codegen::set_ini_overrides(vec![(
        "open_basedir".to_string(),
        "/definitely/allowed".to_string(),
    )]);
    let output = compile_and_run_capture_merged(
        r#"<?php
var_dump(tidy_parse_file('/definitely/outside.html'));
$tidy = new tidy;
var_dump($tidy->parseFile('/definitely/outside.html'));
try {
    new tidy('/definitely/outside.html');
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    let out = format!("{}{}", output.stdout, output.stderr);
    assert_eq!(
        out.matches("open_basedir restriction in effect").count(),
        3,
        "unexpected open_basedir warnings:\n{out}"
    );
    assert_eq!(
        out.matches("Cannot load \"/definitely/outside.html\" into memory")
            .count(),
        3,
        "unexpected load diagnostics:\n{out}"
    );
}

/// A node wrapper keeps its document alive after the `tidy` object that created it is
/// released, matching php-src's document reference count.
#[test]
fn test_tidy_node_outlives_its_document_object() {
    if skip_without_tidy_native("test_tidy_node_outlives_its_document_object") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
function makeNode() {
    $tidy = tidy_parse_string('<b>x</b>');
    return $tidy->body();
}
$node = makeNode();
echo $node->child[0]->child[0]->value, "\n";
"#,
    );
    assert_eq!(out.trim_end(), "x");
}

/// A runtime-computed class name still cannot bypass `tidyNode`'s private constructor.
#[test]
fn test_tidy_node_private_constructor_rejects_dynamic_new() {
    if skip_without_tidy_native("test_tidy_node_private_constructor_rejects_dynamic_new") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$class = tidyNode::class;
try {
    new $class;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out.trim_end(),
        "Call to private tidyNode::__construct() from global scope"
    );
}

/// A subclass that overrides the constructor without calling the parent still gets a
/// usable document, matching php-src's create-object allocation.
#[test]
fn test_tidy_subclass_skipping_parent_constructor_still_works() {
    if skip_without_tidy_native("test_tidy_subclass_skipping_parent_constructor_still_works") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
class TidyChild extends tidy {
    public function __construct() {}
}
$tidy = new TidyChild;
$tidy->parseString('<b>x</b>');
echo $tidy->body()->child[0]->child[0]->value, "\n";
"#,
    );
    assert_eq!(out.trim_end(), "x");
}

/// In quick repair, a failed configuration load skips the encoding attempt (php-src's
/// `else if`) but still parses and returns the repaired string.
#[test]
fn test_tidy_repair_skips_encoding_after_config_failure() {
    if skip_without_tidy_native("test_tidy_repair_skips_encoding_after_config_failure") {
        return;
    }
    let _restore = IniRestore;
    elephc::codegen::set_ini_overrides(vec![(
        "open_basedir".to_string(),
        "/definitely/allowed".to_string(),
    )]);
    let output = compile_and_run_capture_merged(
        r#"<?php
$result = tidy_repair_string('<B>testing</I>', '/definitely/outside.ini', 'invalid-encoding-name');
var_dump($result !== false);
"#,
    );
    let out = format!("{}{}", output.stdout, output.stderr);
    assert_eq!(
        out.matches("open_basedir restriction in effect").count(),
        1,
        "unexpected open_basedir warnings:\n{out}"
    );
    assert!(
        !out.contains("Could not set encoding"),
        "encoding must be skipped after a config failure:\n{out}"
    );
    assert!(out.contains("bool(true)"), "repair must still succeed:\n{out}");
}

/// Reparsing a document invalidates node wrappers from the previous tree instead of
/// leaving them pointing at storage libtidy has already released.
#[test]
fn test_tidy_reparse_invalidates_previous_nodes() {
    if skip_without_tidy_native("test_tidy_reparse_invalidates_previous_nodes") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$tidy = tidy_parse_string('<b>old</b>');
$old = $tidy->body();
$tidy->parseString('<i>new</i>');
var_dump($old->name, $old->getParent());
echo $tidy->body()->child[0]->child[0]->value, "\n";
"#,
    );
    assert_eq!(out.trim_end(), "string(4) \"body\"\nNULL\nnew");
}

/// A successfully parsed document whose output is empty updates `tidy::$value` to the
/// empty string, while a never-parsed object keeps the property `null`.
#[test]
fn test_tidy_empty_output_updates_the_value_property() {
    if skip_without_tidy_native("test_tidy_empty_output_updates_the_value_property") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$tidy = tidy_parse_string('', ['show-body-only' => true]);
var_dump($tidy->value);
var_dump((new tidy)->value);
"#,
    );
    assert_eq!(out.trim_end(), "string(0) \"\"\nNULL");
}

/// A literal `eval()` fragment that registers `ob_tidyhandler` resolves the same AOT prelude
/// callback as compiled code, and its FINAL flush still runs libtidy before the outer buffer is
/// captured. This exercises the eval output-handler bridge rather than `tidy.clean_output`.
#[test]
fn test_tidy_output_handler_inside_eval() {
    if skip_without_tidy_native("test_tidy_output_handler_inside_eval") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
ob_start();
eval('
    ob_start("ob_tidyhandler");
    echo "<B>testing</I>";
    ob_end_flush();
');
$repaired = ob_get_clean();
echo $repaired;
"#,
    );
    assert_eq!(
        out.trim_end(),
        "<!DOCTYPE html>\n<html>\n<head>\n<title></title>\n</head>\n<body>\n<b>testing</b>\n</body>\n</html>"
    );
}

/// `ob_tidyhandler` is a real output callback and `ob_start` can register it by name.
#[test]
fn test_tidy_output_handler_buffer() {
    if skip_without_tidy_native("test_tidy_output_handler_buffer") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
ob_start('ob_tidyhandler');
echo '<B>testing</I>';
ob_end_flush();
"#,
    );
    assert!(
        out.contains("<body>\n<b>testing</b>\n</body>"),
        "output handler did not repair the buffer:\n{out}"
    );
}

/// The float-to-int warning inside configuration coercion is an engine `E_WARNING`, not the
/// userland `E_USER_WARNING`; a custom handler must observe severity 2 exactly as php-src does.
#[test]
fn test_tidy_float_to_int_warning_uses_engine_severity() {
    if skip_without_tidy_native("test_tidy_float_to_int_warning_uses_engine_severity") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $severity, string $message): bool {
    echo $severity, ':', $message, "\n";
    return true;
});
$tidy = new tidy();
$tidy->parseString('<p>x</p>', ['custom-tags' => NAN]);
restore_error_handler();
echo "ok\n";
"#,
    );
    assert_eq!(
        out,
        "2:The float NAN is not representable as an int, cast occurred\nok\n"
    );
}

/// A tidy function name stored in a string is a real runtime callable, both through a variable
/// call and `call_user_func()`, and its literal alone links the prelude.
#[test]
fn test_tidy_function_name_is_a_runtime_callable() {
    if skip_without_tidy_native("test_tidy_function_name_is_a_runtime_callable") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$parse = 'tidy_parse_string';
$first = $parse('<p>x</p>');
$second = call_user_func('tidy_parse_string', '<p>y</p>');
echo get_class($first), '|', get_class($second), '|', $first->body()->child[0]->name, "\n";
"#,
    );
    assert_eq!(out, "tidy|tidy|p\n");
}

/// A fixed `new tidyNode()` from a class scope names that scope in the runtime Error, matching
/// php-src's `zend_throw_error` wording instead of always saying "global scope".
#[test]
fn test_tidy_node_private_constructor_names_the_calling_scope() {
    if skip_without_tidy_native("test_tidy_node_private_constructor_names_the_calling_scope") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
class TidyNodeFactory {
    public static function make(): void {
        try {
            new tidyNode();
        } catch (Error $error) {
            echo $error->getMessage(), "\n";
        }
    }
}
TidyNodeFactory::make();
"#,
    );
    assert_eq!(
        out,
        "Call to private tidyNode::__construct() from scope TidyNodeFactory\n"
    );
}

/// The prelude constructor itself names an eval caller's class scope when eval allocates a
/// `tidyNode` directly, rather than hard-coding "global scope".
#[test]
fn test_tidy_node_eval_private_constructor_names_the_eval_scope() {
    if skip_without_tidy_native("test_tidy_node_eval_private_constructor_names_the_eval_scope") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$message = eval('
class EvalTidyNodeFactory {
    public static function make(): string {
        try {
            new tidyNode;
        } catch (Error $error) {
            return $error->getMessage();
        }
        return "no-error";
    }
}
return EvalTidyNodeFactory::make();
');
echo $message, "\n";
"#,
    );
    assert_eq!(
        out,
        "Call to private tidyNode::__construct() from scope EvalTidyNodeFactory\n"
    );
}

/// `cleanRepair()` prunes nodes through libtidy's cleaner, so every wrapper minted
/// against the pre-clean tree must be invalidated before the call. The invalidated
/// handles then answer through the existing absent-handle fallbacks instead of
/// dereferencing a freed `TidyNode*`, and `hasChildren()` uses the live bridge
/// predicate rather than the constructor's snapshot.
#[test]
fn test_tidy_clean_repair_invalidates_held_nodes() {
    if skip_without_tidy_native("test_tidy_clean_repair_invalidates_held_nodes") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$tidy = new tidy();
$tidy->parseString('<html><body><p>x</p></body></html>');
$body = $tidy->body();
$child = $body->child[0];
$cleaned = $tidy->cleanRepair();
var_dump($cleaned);
var_dump($body->hasChildren());
var_dump($child->isText());
var_dump($child->hasChildren());
var_dump($child->getParent());
var_dump((string) $child);
"#,
    );
    assert_eq!(
        out.trim_end(),
        "bool(true)\nbool(false)\nbool(false)\nbool(false)\nNULL\nstring(0) \"\""
    );
}

/// The literal-eval scanner matches PHP keywords case-insensitively, so an eval fragment
/// whose only tidy reference is `NEW TIDY()` still links the prelude and runs.
#[test]
fn test_tidy_literal_eval_uppercase_new_keyword() {
    if skip_without_tidy_native("test_tidy_literal_eval_uppercase_new_keyword") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
eval('$t = NEW TIDY(); $t->parseString("<b>x</b>"); echo get_class($t), ":", $t->body()->name;');
"#,
    );
    assert_eq!(out, "tidy:body");
}

/// A non-string `$encoding` throws PHP's argument `TypeError` instead of being cast.
#[test]
fn test_tidy_invalid_encoding_argument_throws_type_error() {
    if skip_without_tidy_native("test_tidy_invalid_encoding_argument_throws_type_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
try {
    tidy_parse_string('<p>x</p>', null, []);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "tidy_parse_string(): Argument #3 ($encoding) must be of type ?string, array given\n"
    );
}

/// A non-array/string/null `$config` throws PHP's argument `TypeError` instead of being cast.
#[test]
fn test_tidy_invalid_config_argument_throws_type_error() {
    if skip_without_tidy_native("test_tidy_invalid_config_argument_throws_type_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
try {
    tidy_parse_string('<p>x</p>', new stdClass);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "tidy_parse_string(): Argument #2 ($config) must be of type array|string|null, stdClass given\n"
    );
}

/// Re-calling `tidy::__construct()` releases the previous bridge document before replacing
/// the handle, and the reinitialized object parses normally.
#[test]
fn test_tidy_constructor_can_be_called_again() {
    if skip_without_tidy_native("test_tidy_constructor_can_be_called_again") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$tidy = new tidy();
$tidy->parseString('<b>old</b>');
$tidy->__construct();
$tidy->parseString('<b>new</b>');
echo $tidy->body()->child[0]->child[0]->value, "\n";
"#,
    );
    assert_eq!(out, "new\n");
}
