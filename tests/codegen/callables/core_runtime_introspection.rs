//! Purpose:
//! End-to-end AOT tests for Core declaration, extension, and include introspection.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` integration harness.
//!
//! Key details:
//! - Multi-file fixtures verify canonical include order and the required-files alias.
//! - Dynamic extension names exercise the runtime case-insensitive comparison path.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Verifies AOT backtraces expose the active function, current arguments, options, and limits.
#[test]
fn test_core_debug_backtrace_aot_current_frame() {
    let out = compile_and_run(
        r#"<?php
        function traced_core_frame(int $value): void {
            $value = 9;
            $trace = DeBuG_BaCkTrAcE();
            echo count($trace), ":", $trace[0]["function"], ":";
            echo $trace[0]["args"][0], ":", $trace[0]["line"] > 0 ? "L:" : "bad:";
            $ignored = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 1);
            echo isset($ignored[0]["args"]) ? "bad:" : "ignored:";
            echo count(debug_backtrace(0, -1)), "|";
            debug_print_backtrace(0, 1);
        }
        traced_core_frame(7);
        "#,
    );
    assert!(
        out.starts_with("1:traced_core_frame:9:L:ignored:0|#0 "),
        "unexpected backtrace output: {out}"
    );
    assert!(
        out.ends_with(": traced_core_frame(9)\n"),
        "unexpected backtrace output: {out}"
    );
}

/// Verifies printed AOT traces walk nested frames and format every PHP value category.
#[test]
fn test_core_debug_print_backtrace_aot_nested_arguments() {
    let out = compile_and_run(
        r#"<?php
        class CorePrintedTraceObject {}
        function core_print_inner(
            int $number,
            string $text,
            bool $flag,
            mixed $nothing,
            array $items,
            CorePrintedTraceObject $object,
            callable $callback
        ): void {
            debug_print_backtrace(0, 2);
        }
        function core_print_outer(
            int $number,
            string $text,
            bool $flag,
            mixed $nothing,
            array $items,
            CorePrintedTraceObject $object,
            callable $callback
        ): void {
            core_print_inner($number, $text, $flag, $nothing, $items, $object, $callback);
        }
        $callback = function (): void {};
        core_print_outer(7, "a\\b\nc", true, null, [1], new CorePrintedTraceObject(), $callback);
        "#,
    );
    let arguments =
        "(7, 'a\\\\b\\nc', true, NULL, Array, Object(CorePrintedTraceObject), Object(Closure))";
    assert!(
        out.contains(&format!(": core_print_inner{arguments}\n")),
        "unexpected inner backtrace output: {out}"
    );
    assert!(
        out.contains(&format!(": core_print_outer{arguments}\n")),
        "unexpected outer backtrace output: {out}"
    );
    assert_eq!(out.lines().count(), 2, "unexpected frame count: {out}");
}

/// Verifies printed AOT traces trim defaults and decode typed variadic collectors safely.
#[test]
fn test_core_debug_print_backtrace_aot_optional_and_variadic_arguments() {
    let out = compile_and_run(
        r#"<?php
        function print_core_optional(int $first = 1, int $second = 2, int ...$rest): void {
            debug_print_backtrace(0, 1);
        }
        function print_core_strings(string ...$rest): void {
            debug_print_backtrace(0, 1);
        }
        function print_core_mixed(mixed ...$rest): void {
            debug_print_backtrace(0, 1);
        }
        function print_core_surplus(int $first): void {
            debug_print_backtrace(0, 1);
        }
        print_core_optional();
        print_core_optional(3);
        print_core_optional(3, 4, 5, 6);
        print_core_strings("left", "right");
        print_core_mixed(7, "value", 1.5);
        print_core_surplus(8, "extra");
        "#,
    );
    for expected in [
        ": print_core_optional()",
        ": print_core_optional(3)",
        ": print_core_optional(3, 4, 5, 6)",
        ": print_core_strings('left', 'right')",
        ": print_core_mixed(7, 'value', 1.5)",
        ": print_core_surplus(8, 'extra')",
    ] {
        assert!(out.contains(expected), "missing {expected:?} in {out}");
    }
    assert_eq!(out.lines().count(), 6, "unexpected frame count: {out}");
}

/// Verifies nested AOT function and method activations expose live outer frames and receivers.
#[test]
fn test_core_debug_backtrace_aot_nested_frames_and_objects() {
    let out = compile_and_run(
        r#"<?php
        function inner_core_trace(int $value): void {
            $trace = debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT);
            echo count($trace), ":";
            echo $trace[0]["function"], ":", $trace[0]["args"][0], ":";
            echo $trace[1]["function"], ":", $trace[1]["args"][0], ":";
            echo $trace[0]["line"] > 0 && $trace[1]["line"] > 0 ? "L|" : "bad|";
        }
        function outer_core_trace(int $value): void {
            inner_core_trace($value + 1);
        }
        class CoreTraceProbe {
            public function inner(int $value): void {
                $trace = debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT, 2);
                echo count($trace), ":";
                echo $trace[0]["class"], $trace[0]["type"], $trace[0]["function"], ":";
                echo $trace[0]["object"] === $this ? "O:" : "bad:";
                echo $trace[1]["class"], $trace[1]["type"], $trace[1]["function"], ":";
                echo $trace[1]["object"] === $this ? "O|" : "bad|";
                $without = debug_backtrace(0, 1);
                echo isset($without[0]["object"]) ? "bad" : "clean";
            }
            public function outer(int $value): void {
                $this->inner($value + 1);
            }
        }
        outer_core_trace(4);
        $probe = new CoreTraceProbe();
        $probe->outer(8);
        "#,
    );
    assert_eq!(
        out,
        "2:inner_core_trace:5:outer_core_trace:4:L|2:CoreTraceProbe->inner:O:CoreTraceProbe->outer:O|clean"
    );
}

/// Verifies AOT backtrace arguments contain only values actually supplied by the caller.
#[test]
fn test_core_debug_backtrace_aot_optional_and_variadic_arguments() {
    let out = compile_and_run(
        r#"<?php
        function inspect_core_arguments(int $first = 1, int $second = 2, int ...$rest): void {
            $args = debug_backtrace()[0]["args"];
            echo count($args);
            foreach ($args as $arg) {
                echo ":", $arg;
            }
            echo "|";
        }
        function inspect_core_strings(string ...$rest): void {
            $args = debug_backtrace()[0]["args"];
            echo count($args), ":", $args[0], ":", $args[1], "|";
        }
        function inspect_core_mixed(mixed ...$rest): void {
            $args = debug_backtrace()[0]["args"];
            echo count($args), ":", $args[0], ":", $args[1], ":", $args[2], "|";
        }
        function inspect_core_surplus(int $first): void {
            $args = debug_backtrace()[0]["args"];
            echo count($args), ":", $args[0], ":", $args[1], "|";
        }
        inspect_core_arguments();
        inspect_core_arguments(3);
        inspect_core_arguments(3, 4, 5, 6);
        inspect_core_strings("left", "right");
        inspect_core_mixed(7, "value", 1.5);
        inspect_core_surplus(8, "extra");
        "#,
    );
    assert_eq!(
        out,
        "0|1:3|4:3:4:5:6|2:left:right|3:7:value:1.5|2:8:extra|"
    );
}

/// Verifies the process-local reporting mask returns its previous value and accepts null as a query.
#[test]
fn test_core_error_reporting_aot_get_set_and_null_query() {
    let out = compile_and_run(
        r#"<?php
        echo error_reporting(), ":";
        echo error_reporting(513), ":";
        echo error_reporting(), ":";
        echo error_reporting(null);
        "#,
    );
    assert_eq!(out, "30719:30719:513:513");
}

/// Verifies `E_ALL` and the initial reporting mask follow the selected PHP profile together.
#[test]
fn test_core_error_reporting_aot_tracks_php_profile() {
    let source = "<?php echo E_ALL, ':', error_reporting();";
    assert_eq!(
        compile_and_run_with_php_version(source, PhpVersion::Php84),
        "32767:32767"
    );
    assert_eq!(
        compile_and_run_with_php_version(source, PhpVersion::Php85),
        "30719:30719"
    );
}

/// Verifies nested error-handler registration, callback arguments, masks, and restoration.
#[test]
fn test_core_error_handler_aot_dispatch_and_restore() {
    let out = compile_and_run(
        r#"<?php
        function first_core_handler(int $level, string $message, string $file, int $line): bool {
            echo "F", $level, ":", $message, ":", $file !== "" ? "P" : "p", ":", $line > 0 ? "L" : "l", "|";
            return true;
        }
        function second_core_handler(int $level, string $message): bool {
            echo "S", $level, ":", $message, "|";
            return true;
        }
        echo is_null(set_error_handler("first_core_handler", E_USER_WARNING)) ? "N|" : "bad|";
        $previous = set_error_handler("second_core_handler", E_USER_WARNING);
        echo $previous === "first_core_handler" ? "P|" : "bad|";
        trigger_error("second", E_USER_WARNING);
        trigger_error("masked", E_USER_NOTICE);
        echo restore_error_handler() ? "R|" : "bad|";
        user_error("first", E_USER_WARNING);
        "#,
    );
    assert_eq!(out, "N|P|S512:second|R|F512:first:P:L|");
}

/// Verifies returning exact false delegates to PHP's default path while other values suppress it.
#[test]
fn test_core_error_handler_aot_exact_false_fallback() {
    let out = compile_and_run(
        r#"<?php
        function false_core_handler(): bool { echo "H|"; return false; }
        function null_core_handler(): mixed { echo "N|"; return null; }
        error_reporting(0);
        set_error_handler("false_core_handler", E_USER_WARNING);
        echo trigger_error("hidden", E_USER_WARNING) ? "T|" : "bad|";
        set_error_handler("null_core_handler", E_USER_WARNING);
        echo trigger_error("suppressed", E_USER_WARNING) ? "T" : "bad";
        "#,
    );
    assert_eq!(out, "H|T|N|T");
}

/// Verifies AOT rejects engine error levels through PHP's catchable `ValueError` path.
#[test]
fn test_core_trigger_error_aot_rejects_non_user_level() {
    let out = compile_and_run(
        r#"<?php
        try {
            trigger_error("invalid", E_WARNING);
        } catch (ValueError $error) {
            echo $error->getMessage();
        }
        "#,
    );
    assert_eq!(
        out,
        "trigger_error(): Argument #2 ($error_level) must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED"
    );
}

/// Verifies a suppressing AOT handler may recover from `E_USER_ERROR` like PHP.
#[test]
fn test_core_trigger_error_aot_handler_suppresses_user_fatal() {
    let out = compile_and_run(
        r#"<?php
        function suppress_core_fatal(int $level, string $message): bool {
            echo $level, ":", $message, "|";
            return true;
        }
        set_error_handler("suppress_core_fatal", E_USER_ERROR);
        echo trigger_error("handled", E_USER_ERROR) ? "alive" : "bad";
        "#,
    );
    assert_eq!(out, "256:handled|alive");
}

/// Verifies an unhandled AOT `E_USER_ERROR` emits a located diagnostic and terminates.
#[test]
fn test_core_trigger_error_aot_unhandled_user_error_is_fatal() {
    let out = compile_and_run_capture("<?php\ntrigger_error(\"boom\", E_USER_ERROR);\necho 'unreachable';");
    assert!(!out.success, "E_USER_ERROR must terminate the process");
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Fatal error: boom in ")
            && out.stderr.contains(" on line 2\n"),
        "unexpected stderr: {:?}",
        out.stderr
    );
}

/// Verifies an unhandled eval `E_USER_ERROR` uses the same fatal path as AOT code.
#[test]
fn test_core_trigger_error_eval_unhandled_user_error_is_fatal() {
    let out = compile_and_run_capture(
        r#"<?php
        $source = 'trigger_error("boom", E_USER_ERROR); $runtime = ' . $argc . ';';
        eval($source);
        echo "unreachable";
        "#,
    );
    assert!(!out.success, "eval E_USER_ERROR must terminate the process");
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Fatal error: boom in ")
            && out.stderr.contains(" on line ")
            && !out.stderr.contains("eval() runtime failed"),
        "unexpected stderr: {:?}",
        out.stderr
    );
}

/// Verifies exception-handler registration returns and restores the prior PHP callback value.
#[test]
fn test_core_exception_handler_aot_registration_stack() {
    let out = compile_and_run(
        r#"<?php
        function first_exception_handler(Throwable $exception): void {}
        function second_exception_handler(Throwable $exception): void {}
        echo is_null(set_exception_handler("first_exception_handler")) ? "N|" : "bad|";
        echo set_exception_handler("second_exception_handler") === "first_exception_handler" ? "P|" : "bad|";
        echo restore_exception_handler() ? "R|" : "bad|";
        echo set_exception_handler(null) === "first_exception_handler" ? "F|" : "bad|";
        echo restore_exception_handler() ? "R" : "bad";
        "#,
    );
    assert_eq!(out, "N|P|R|F|R");
}

/// Verifies an uncaught AOT Throwable is delivered once to the active terminal handler.
#[test]
fn test_core_exception_handler_aot_receives_uncaught_throwable() {
    let out = compile_and_run(
        r#"<?php
        function terminal_exception_handler(Throwable $exception): void {
            echo get_class($exception), ":", $exception->getMessage();
        }
        set_exception_handler("terminal_exception_handler");
        throw new Exception("boom");
        echo "unreachable";
        "#,
    );
    assert_eq!(out, "Exception:boom");
}

/// Verifies a handler registered by dynamic eval remains live for terminal AOT dispatch.
#[test]
fn test_core_exception_handler_eval_registration_reaches_aot_terminal_dispatch() {
    let out = compile_and_run(
        r#"<?php
        function install_eval_exception_handler(int $runtime): void {
            $source = 'function eval_terminal_handler($exception) {'
                . ' echo get_class($exception), ":", $exception->getMessage(); }'
                . ' set_exception_handler("eval_terminal_handler");'
                . ' $runtime = ' . $runtime . ';';
            eval($source);
        }
        install_eval_exception_handler($argc);
        throw new Exception("eval-boom");
        echo "unreachable";
        "#,
    );
    assert_eq!(out, "Exception:eval-boom");
}

/// Verifies an outer AOT catch wins before an eval-installed terminal handler and can restore it.
#[test]
fn test_core_exception_handler_eval_registration_preserves_outer_aot_catch() {
    let out = compile_and_run(
        r#"<?php
        function must_not_run(Throwable $exception): void { echo "handler"; }
        try {
            $source = 'set_exception_handler("must_not_run");'
                . ' throw new Exception("caught");'
                . ' $runtime = ' . $argc . ';';
            eval($source);
        } catch (Exception $exception) {
            echo $exception->getMessage(), "|";
        }
        echo restore_exception_handler() ? "restored" : "bad";
        "#,
    );
    assert_eq!(out, "caught|restored");
}

/// Verifies AOT and dynamic eval observe and replace one process-wide reporting mask.
#[test]
fn test_core_error_reporting_crosses_aot_eval_boundary() {
    let out = compile_and_run(
        r#"<?php
        echo error_reporting(0), ":";
        $source = 'echo error_reporting(), ":";'
            . ' echo error_reporting(513), ":";'
            . ' $runtime = ' . $argc . ';';
        eval($source);
        echo error_reporting();
        "#,
    );
    assert_eq!(out, "30719:0:0:513");
}

/// Verifies native and eval error handlers share dispatch, callback results, and restoration.
#[test]
fn test_core_error_handler_crosses_aot_eval_boundary() {
    let out = compile_and_run(
        r#"<?php
        function aot_core_error_handler(int $level, string $message): bool {
            echo "A", $level, ":", $message, "|";
            return true;
        }
        set_error_handler("aot_core_error_handler", E_USER_WARNING);
        $install = 'function eval_core_error_handler($level, $message) {'
            . ' echo "E" . $level . ":" . $message . "|"; return true; }'
            . ' $previous = set_error_handler("eval_core_error_handler", E_USER_WARNING);'
            . ' echo $previous === "aot_core_error_handler" ? "P|" : "bad|";'
            . ' $runtime = ' . $argc . ';';
        eval($install);
        trigger_error("outside", E_USER_WARNING);
        echo restore_error_handler() ? "R|" : "bad|";
        $inside = 'trigger_error("inside", E_USER_WARNING);'
            . ' echo restore_error_handler() ? "r" : "bad";'
            . ' $runtime = ' . $argc . ';';
        eval($inside);
        "#,
    );
    assert_eq!(out, "P|E512:outside|R|A512:inside|r");
}

/// Verifies flat and categorized constant inventories contain builtin and user values.
#[test]
fn test_core_get_defined_constants_aot_values_and_categories() {
    let out = compile_and_run(
        r#"<?php
        const USER_CORE_VALUE = 42;
        $flat = GeT_DeFiNeD_CoNsTaNtS();
        $categorized = get_defined_constants(categorize: true);
        echo $flat["E_USER_WARNING"], ":", $flat["USER_CORE_VALUE"], ":";
        echo $categorized["Core"]["PHP_INT_SIZE"], ":";
        echo $categorized["user"]["USER_CORE_VALUE"];
        "#,
    );
    assert_eq!(out, "512:42:8:42");
}

/// Verifies scope introspection captures initialized parameters and locals at the call site.
#[test]
fn test_core_get_defined_vars_aot_current_scope() {
    let out = compile_and_run(
        r#"<?php
        function inspect_scope(int $argument): void {
            $local = 7;
            $vars = GeT_DeFiNeD_VaRs();
            $callbackVars = call_user_func("get_defined_vars");
            $arrayCallbackVars = call_user_func_array("GET_DEFINED_VARS", []);
            $later = 11;
            echo $vars["argument"], ":", $vars["local"], ":";
            echo isset($vars["later"]) ? "bad:" : "clean:";
            echo $callbackVars["argument"], ":", $callbackVars["local"], ":";
            echo $arrayCallbackVars["argument"], ":", $arrayCallbackVars["local"];
        }
        inspect_scope(5);
        "#,
    );
    assert_eq!(out, "5:7:clean:5:7:5:7");
}

/// Verifies internal and user function inventories are populated from compiler metadata.
#[test]
fn test_core_get_defined_functions_aot_inventories() {
    let out = compile_and_run(
        r#"<?php
        function UserThing(): void {}
        $defined = GeT_DeFiNeD_FuNcTiOnS();
        $includeDisabled = get_defined_functions(exclude_disabled: false);
        echo in_array("strlen", $defined["internal"]) ? "I" : "i";
        echo in_array("userthing", $defined["user"]) ? "U" : "u";
        echo $defined === $includeDisabled ? "S" : "x";
        echo count($defined) === 2 ? "2" : "x";
        "#,
    );
    assert_eq!(out, "IUS2");
}

/// Verifies AOT object introspection exposes public, protected, and private mangled keys.
#[test]
fn test_core_get_mangled_object_vars_aot_visibility_keys() {
    let out = compile_and_run(
        r#"<?php
        class CoreMangledBag {
            public int $open = 1;
            protected int $guarded = 2;
            private int $secret = 3;
        }
        $vars = GeT_MaNgLeD_ObJeCt_VaRs(object: new CoreMangledBag());
        $protected = chr(0) . "*" . chr(0) . "guarded";
        $private = chr(0) . "CoreMangledBag" . chr(0) . "secret";
        echo count($vars), ":", $vars["open"], ":", $vars[$protected], ":", $vars[$private];
        "#,
    );
    assert_eq!(out, "3:1:2:3");
}

/// Verifies Core extension lookup accepts a dynamic case-insensitive name and reports all names.
#[test]
fn test_core_get_extension_funcs_aot_dynamic_core_name() {
    let out = compile_and_run(
        r#"<?php
        $extension = "cOrE";
        $functions = get_extension_funcs(extension: $extension);
        echo count($functions), ":";
        echo $functions[0] === "class_alias" ? "D" : "d";
        echo get_extension_funcs("not-loaded") === false ? "F" : "f";
        "#,
    );
    assert_eq!(out, "59:DF");
}

/// Verifies both include-introspection aliases expose the resolved canonical script manifest.
#[test]
fn test_core_get_included_files_aot_manifest_and_alias() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
                include "library.php";
                require "required.php";
                $included = get_included_files();
                $required = GET_REQUIRED_FILES();
                echo count($included), ":", count($required), ":";
                echo basename($included[0]), ":", basename($included[1]), ":";
                echo basename($included[2]), ":";
                echo $included === $required ? "same" : "different";
                "#,
            ),
            ("library.php", "<?php function included_helper(): int { return 1; }"),
            ("required.php", "<?php function required_helper(): int { return 2; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "3:3:main.php:library.php:required.php:same");
}

/// Verifies standard, implicit-context, live, and closed resources remain enumerable by PHP id.
#[test]
fn test_core_get_resources_aot_inventory_and_filters() {
    let out = compile_and_run(
        r#"<?php
        echo count(GeT_ReSoUrCeS()), ":";
        $first = fopen("php://memory", "w+");
        $first_id = get_resource_id($first);
        $open = get_resources(type: "stream");
        echo count(get_resources()), ":";
        echo count($open), ":";
        echo array_key_exists($first_id, $open) ? "O:" : "bad:";
        fclose($first);
        $closed = get_resources("Unknown");
        echo count($closed), ":";
        echo get_resource_id($closed[$first_id]), ":";
        echo get_resource_type($closed[$first_id]), ":";
        try {
            get_resources("Stream");
        } catch (ValueError $error) {
            echo $error->getMessage(), ":";
        }
        $second = fopen("php://memory", "w+");
        echo count(get_resources()), ":";
        echo get_resource_id($second) > $first_id ? "N" : "bad";
        fclose($second);
        "#,
    );
    assert_eq!(
        out,
        "3:5:4:O:1:5:Unknown:get_resources(): Argument #1 ($type) must be a valid resource type:6:N"
    );
}
