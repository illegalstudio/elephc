//! Purpose:
//! Interpreter tests for time, system, SPL, environment, host, protocol, and IP builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases isolate platform-facing builtins behind deterministic fake assertions where possible.

use super::super::*;
use super::support::*;

/// Verifies eval `error_reporting()` shares PHP's query/update semantics and constants.
#[test]
fn execute_program_dispatches_error_reporting_builtin() {
    let program = parse_fragment(
        br#"echo error_reporting(); echo ":";
echo error_reporting(0); echo ":";
echo error_reporting(); echo ":";
echo call_user_func("error_reporting", E_ALL & ~E_DEPRECATED); echo ":";
echo error_reporting();
return error_reporting(null);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "30719:30719:0:0:22527");
    assert_eq!(values.get(result), FakeValue::Int(22527));
}

/// Verifies eval E_STRICT reads emit a mask-aware PHP 8.4+ deprecation.
#[test]
fn execute_program_deprecates_e_strict_reads() {
    let program = parse_fragment(br#"return E_STRICT;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2048));
    assert_eq!(
        values.warnings,
        vec!["\nDeprecated: Constant E_STRICT is deprecated since 8.4, the error level was removed"]
    );
}

/// Verifies eval `setlocale()` tries array, variadic, and callable candidates in PHP order.
#[test]
fn execute_program_dispatches_setlocale_builtin() {
    let program = parse_fragment(
        br#"echo setlocale(LC_ALL, ["__elephc_invalid_locale__", "C"]); echo ":";
echo setlocale(LC_ALL, "__elephc_invalid_locale__", "C"); echo ":";
echo call_user_func("setlocale", LC_ALL, 0);
return setlocale(LC_ALL, ["__elephc_invalid_locale__"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "C:C:C");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies eval zero-argument system builtins return native-compatible values.
#[test]
fn execute_program_dispatches_zero_arg_system_builtins() {
    let program = parse_fragment(
        br#"echo time() > 1000000000 ? "time" : "bad"; echo ":";
echo phpversion(); echo ":";
echo sys_get_temp_dir(); echo ":";
echo strlen(getcwd()) > 0 ? "cwd" : "bad"; echo ":";
echo call_user_func("time") > 1000000000 ? "call-time" : "bad"; echo ":";
echo call_user_func("phpversion"); echo ":";
echo call_user_func_array("getcwd", []) !== "" ? "call-cwd" : "bad"; echo ":";
echo call_user_func_array("sys_get_temp_dir", []); echo ":";
echo function_exists("time"); echo function_exists("phpversion"); echo function_exists("getcwd");
return function_exists("sys_get_temp_dir");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        format!(
            "time:{}:/tmp:cwd:call-time:{}:call-cwd:/tmp:111",
            crate::eval_php_profile::eval_php_version_string(),
            crate::eval_php_profile::eval_php_version_string()
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `opcache_get_configuration()` builds the compile-target OPcache
/// configuration array (directives + version + blacklist) with the same typed,
/// normalized 8.5 defaults the native prelude renders, and that `function_exists`
/// reports the prelude-provided name.
#[test]
fn execute_program_dispatches_opcache_get_configuration_builtin() {
    let program = parse_fragment(
        br#"$c = opcache_get_configuration();
echo $c['version']['opcache_product_name']; echo ':';
echo $c['version']['version']; echo ':';
echo $c['directives']['opcache.jit']; echo ':';
echo $c['directives']['opcache.memory_consumption']; echo ':';
echo $c['directives']['opcache.optimization_level']; echo ':';
echo $c['directives']['opcache.jit_hot_loop']; echo ':';
echo $c['directives']['opcache.enable'] ? '1' : '0';
echo $c['directives']['opcache.enable_cli'] ? '1' : '0'; echo ':';
echo count($c['directives']); echo ':';
echo count($c['blacklist']);
return function_exists('opcache_get_configuration');"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Zend OPcache:8.5.0:disable:134217728:2147401727:61:10:54:0"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `opcache_reset()` returns the compile-time cache-enabled boolean using
/// the CLI default (the eval interpreter has no runtime SAPI), which is `false` —
/// matching reference-PHP `php script.php` where `opcache.enable_cli` is off — and that
/// `function_exists` reports the prelude-provided name. Also exercises the
/// dynamic-callable dispatch path (`call_user_func`).
#[test]
fn execute_program_dispatches_opcache_reset_builtin() {
    let program = parse_fragment(
        br#"echo opcache_reset() ? '1' : '0'; echo ':';
echo call_user_func('opcache_reset') ? '1' : '0'; echo ':';
echo function_exists('opcache_reset') ? '1' : '0';
return opcache_reset();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // CLI default: opcache_reset() disabled -> false ('0'); function_exists -> true ('1').
    assert_eq!(values.output, "0:0:1");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies eval `opcache_get_status()` reports the compile-time cache-enabled state
/// using the CLI default (the eval interpreter has no runtime SAPI), which is disabled,
/// so it returns `false` — matching reference-PHP `php script.php`. The optional
/// `$include_scripts` argument does not change the disabled result, and `function_exists`
/// reports the prelude-provided name. Also exercises the dynamic-callable dispatch path
/// (`call_user_func`).
#[test]
fn execute_program_dispatches_opcache_get_status_builtin() {
    let program = parse_fragment(
        br#"echo opcache_get_status() === false ? '1' : '0'; echo ':';
echo opcache_get_status(false) === false ? '1' : '0'; echo ':';
echo call_user_func('opcache_get_status') === false ? '1' : '0'; echo ':';
echo function_exists('opcache_get_status') ? '1' : '0';
return opcache_get_status();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // CLI default: opcache_get_status() disabled -> false ('1' for the `=== false`
    // checks); function_exists -> true ('1').
    assert_eq!(values.output, "1:1:1:1");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies eval `opcache_is_script_cached()` returns `false` (empty-cache interim) under
/// the CLI default, that `function_exists` reports the prelude-provided name, and that the
/// dynamic-callable dispatch path (`call_user_func`) agrees.
#[test]
fn execute_program_dispatches_opcache_is_script_cached_builtin() {
    let program = parse_fragment(
        br#"echo opcache_is_script_cached('/x') ? '1' : '0'; echo ':';
echo call_user_func('opcache_is_script_cached', '/x') ? '1' : '0'; echo ':';
echo function_exists('opcache_is_script_cached') ? '1' : '0';
return opcache_is_script_cached('/x');"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // Empty-cache interim: not cached -> false ('0'); function_exists -> true ('1').
    assert_eq!(values.output, "0:0:1");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies eval `opcache_invalidate()` returns `false` under the CLI default (disabled
/// cache), accepting both the 1-arg and 2-arg (`$force`) forms, that `function_exists`
/// reports the prelude-provided name, and that the dynamic-callable dispatch path agrees.
#[test]
fn execute_program_dispatches_opcache_invalidate_builtin() {
    let program = parse_fragment(
        br#"echo opcache_invalidate('/x') ? '1' : '0'; echo ':';
echo opcache_invalidate('/x', true) ? '1' : '0'; echo ':';
echo call_user_func('opcache_invalidate', '/x') ? '1' : '0'; echo ':';
echo function_exists('opcache_invalidate') ? '1' : '0';
return opcache_invalidate('/x');"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // CLI default: disabled cache -> false ('0') for 1-arg and 2-arg forms;
    // function_exists -> true ('1').
    assert_eq!(values.output, "0:0:0:1");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies eval `opcache_compile_file()` returns `false` under the CLI default (no runtime
/// compiler, disabled cache), that `function_exists` reports the prelude-provided name, and
/// that the dynamic-callable dispatch path agrees. The eval const-folder has no notice
/// channel, so — unlike the native runtime — it emits no diagnostic.
#[test]
fn execute_program_dispatches_opcache_compile_file_builtin() {
    let program = parse_fragment(
        br#"echo opcache_compile_file('/x') ? '1' : '0'; echo ':';
echo call_user_func('opcache_compile_file', '/x') ? '1' : '0'; echo ':';
echo function_exists('opcache_compile_file') ? '1' : '0';
return opcache_compile_file('/x');"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // CLI default: compile cannot run -> false ('0'); function_exists -> true ('1').
    assert_eq!(values.output, "0:0:1");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies eval `opcache_is_script_cached_in_file_cache()` returns `false` — reference PHP
/// gates it on `opcache.file_cache`, whose C default is NULL, so an unconfigured PHP 8.5.6
/// answers `false` for every path (VERIFIED), and elephc has no file cache at all. Also
/// checks `function_exists` reports the prelude-provided name and that the
/// dynamic-callable dispatch path agrees.
#[test]
fn execute_program_dispatches_opcache_is_script_cached_in_file_cache_builtin() {
    let program = parse_fragment(
        br#"echo opcache_is_script_cached_in_file_cache('/x') ? '1' : '0'; echo ':';
echo call_user_func('opcache_is_script_cached_in_file_cache', '/x') ? '1' : '0'; echo ':';
echo function_exists('opcache_is_script_cached_in_file_cache') ? '1' : '0';
return opcache_is_script_cached_in_file_cache('/x');"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // No file cache configured -> false ('0'); function_exists -> true ('1').
    assert_eq!(values.output, "0:0:1");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies eval `opcache_jit_blacklist()` evaluates to PHP `NULL` (declared `void` in
/// reference PHP 8.5.6, whose call `var_export`s as `NULL` — VERIFIED), that
/// `function_exists` reports the prelude-provided name, and that the dynamic-callable
/// dispatch path agrees. Elephc has no JIT, so the no-op is the whole behavior.
#[test]
fn execute_program_dispatches_opcache_jit_blacklist_builtin() {
    let program = parse_fragment(
        br#"echo opcache_jit_blacklist(function () {}) === null ? '1' : '0'; echo ':';
echo call_user_func('opcache_jit_blacklist', function () {}) === null ? '1' : '0'; echo ':';
echo function_exists('opcache_jit_blacklist') ? '1' : '0';
return opcache_jit_blacklist(function () {});"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // void function -> null ('1' for both `=== null` checks); function_exists -> true ('1').
    assert_eq!(values.output, "1:1:1");
    assert_eq!(values.get(result), FakeValue::Null);
}

/// Verifies eval `date()` formats libc local timestamps and `mktime()` builds them.
#[test]
fn execute_program_dispatches_date_mktime_builtins() {
    let program = parse_fragment(
            br#"$ts = mktime(13, 2, 3, 1, 2, 2024);
echo date("Y-m-d H:i:s", $ts);
echo ":" . date("j-n-G-g-A-a-N-D-M-l-F", $ts);
echo ":" . (date("U", $ts) === strval($ts) ? "U" : "bad");
echo ":" . call_user_func("date", "Y", $ts);
$named = call_user_func_array("mktime", ["hour" => 0, "minute" => 0, "second" => 0, "month" => 1, "day" => 1, "year" => 2000]);
echo ":" . date(format: "Y", timestamp: $named);
$short = call_user_func_array("mktime", ["hour" => 0, "minute" => 0, "second" => 0]);
$positional = mktime(0, 0, 0);
echo ":" . ($short === $positional ? "defaults" : "bad");
echo ":"; echo function_exists("date");
return function_exists("mktime");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "2024-01-02 13:02:03:2-1-13-1-PM-pm-2-Tue-Jan-Tuesday-January:U:2024:2000:defaults:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Silence restores masks on normal and exceptional exits and keeps explicit changes.
#[test]
fn execute_program_error_suppression_restores_masks() {
    let program = parse_fragment(br#"
error_reporting(E_NOTICE);
echo @error_reporting(), ":", error_reporting(), ":";
function change_mask() { error_reporting(E_WARNING); return error_reporting(); }
echo @change_mask(), ":", error_reporting(), ":";
error_reporting(E_NOTICE);
function throw_silenced() { throw new Exception("silenced"); }
try { @throw_silenced(); } catch (Exception $e) {}
@date_default_timezone_set("Invalid/Suppressed");
date_default_timezone_set("Invalid/Notice");
echo error_reporting();
"#).expect("silence fixture parses");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("silence fixture executes");
    assert_eq!(values.output, "0:8:2:2:8");
    assert_eq!(values.warnings.len(), 1);
    assert!(values.warnings[0].contains("Invalid/Notice"));
}

/// Verifies eval UTC calendar builtins and timezone probes are callable-visible.
#[test]
fn execute_program_dispatches_extended_calendar_builtins() {
    let program = parse_fragment(
        br#"echo date_default_timezone_get(); echo ":";
echo date_default_timezone_set("UTC") ? "set" : "bad"; echo ":";
$ts = gmmktime(0, 0, 0, 1, 2, 2024);
echo gmdate("Y-m-d H:i:s", $ts); echo ":";
echo call_user_func("gmdate", "Y", $ts); echo ":";
echo checkdate(2, 29, 2024) ? "leap" : "bad"; echo ":";
echo checkdate(2, 29, 2023) ? "bad" : "common"; echo ":";
$g = getdate(0);
echo $g["year"] . "-" . $g["mon"] . "-" . $g["mday"] . " " . $g["hours"] . ":" . $g["minutes"] . ":" . $g["seconds"];
echo ":" . $g["weekday"] . ":" . $g["month"] . ":" . $g[0];
$l = localtime(0, true);
echo ":" . ($l["tm_year"] + 1900) . "-" . $l["tm_mon"] . "-" . $l["tm_mday"] . " " . $l["tm_hour"];
$n = localtime(0);
$callLocal = call_user_func_array("localtime", ["timestamp" => 0, "associative" => true]);
echo ":" . ($n[5] + 1900) . "-" . $n[4] . ":" . $callLocal["tm_sec"];
echo ":" . function_exists("gmdate") . function_exists("gmmktime") . function_exists("checkdate");
echo function_exists("getdate") . function_exists("localtime") . function_exists("date_default_timezone_get");
return function_exists("date_default_timezone_set");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "UTC:set:2024-01-02 00:00:00:2024:leap:common:1970-1-1 0:0:0:Thursday:January:0:1970-0-1 0:1970-0:0:111111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `strtotime()` parses supported ISO date strings and rejects others.
#[test]
fn execute_program_dispatches_strtotime_builtin() {
    let program = parse_fragment(
        br#"$date = strtotime("2024-06-15");
echo date("Y-m-d H:i:s", $date);
$full = strtotime("2024-06-15 12:30:45");
echo ":" . date("Y-m-d H:i:s", $full);
$short = strtotime("2024-06-15T12:30");
echo ":" . date("Y-m-d H:i:s", $short);
echo ":" . (strtotime("not a date") === false ? "bad" : "wrong");
$call = call_user_func("strtotime", "2024-01-02 03:04:05");
echo ":" . date("Y-m-d H:i:s", $call);
$spread = call_user_func_array("strtotime", ["datetime" => "2024-01-02"]);
echo ":" . date("Y-m-d", $spread);
echo ":" . strtotime("now", 1700000000);
$base = call_user_func_array("strtotime", ["datetime" => "now", "baseTimestamp" => 1700000001]);
echo ":" . $base . ":";
return function_exists("strtotime");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "2024-06-15 00:00:00:2024-06-15 12:30:45:2024-06-15 12:30:00:bad:2024-01-02 03:04:05:2024-01-02:1700000000:1700000001:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `microtime()` preserves PHP's string/float result-mode contract.
#[test]
fn execute_program_dispatches_microtime_builtin() {
    let program = parse_fragment(
        br#"echo is_string(microtime()) ? "now" : "bad"; echo ":";
echo is_string(microtime(as_float: false)) ? "named" : "bad"; echo ":";
echo is_float(call_user_func("microtime", true)) ? "call" : "bad"; echo ":";
echo is_float(call_user_func_array("microtime", ["as_float" => true])) ? "array" : "bad";
echo ":";
return function_exists("microtime");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "now:named:call:array:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `hrtime()`, `http_response_code()`, and `header()` dispatch paths.
#[test]
fn execute_program_dispatches_hrtime_and_http_header_builtins() {
    let program = parse_fragment(
        br#"$parts = hrtime();
echo count($parts) === 2 ? "parts" : "bad"; echo ":";
echo is_int($parts[0]) && is_int($parts[1]) ? "ints" : "bad"; echo ":";
echo hrtime(true) > 0 ? "number" : "bad"; echo ":";
echo http_response_code(); echo ":";
echo http_response_code(404); echo ":";
echo http_response_code(); echo ":";
header("X-Test: 1", true, 201);
echo http_response_code(); echo ":";
echo call_user_func("http_response_code", 202); echo ":";
echo http_response_code(); echo ":";
echo call_user_func_array("header", ["header" => "X-Test: 2", "replace" => false, "response_code" => 204]);
echo http_response_code(); echo ":";
echo function_exists("hrtime") . function_exists("http_response_code") . function_exists("header");
return is_callable("header");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "parts:ints:number:200:200:404:201:201:202:204:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval realpath-cache stubs match elephc's empty-cache runtime view.
#[test]
fn execute_program_dispatches_realpath_cache_builtins() {
    let program = parse_fragment(
        br#"$cache = realpath_cache_get();
echo count($cache) . ":" . realpath_cache_size() . ":";
$call_cache = call_user_func("realpath_cache_get");
echo count($call_cache) . ":";
echo call_user_func_array("realpath_cache_size", []) . ":";
echo function_exists("realpath_cache_get");
return function_exists("realpath_cache_size");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "0:0:0:0:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval stream introspection builtins return native-compatible static lists.
#[test]
fn execute_program_dispatches_stream_introspection_builtins() {
    let program = parse_fragment(
        br#"$wrappers = stream_get_wrappers();
$transports = stream_get_transports();
$filters = stream_get_filters();
echo count($wrappers) . ":" . $wrappers[0] . ":" . $wrappers[5] . ":";
echo count($transports) . ":" . $transports[0] . ":" . $transports[8] . ":";
echo count($filters) . ":" . $filters[2] . ":";
$call_wrappers = call_user_func("stream_get_wrappers");
echo $call_wrappers[10] . ":";
$call_transports = call_user_func_array("stream_get_transports", []);
echo $call_transports[11] . ":";
$call_filters = call_user_func_array("stream_get_filters", []);
echo $call_filters[13] . ":";
echo function_exists("stream_get_wrappers"); echo function_exists("stream_get_transports");
return function_exists("stream_get_filters");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "11:file:https:12:tcp:tlsv1.0:14:string.rot13:glob:tlsv1.3:bzip2.decompress:11"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval stream predicate stubs match elephc's fixed stream metadata behavior.
#[test]
fn execute_program_dispatches_stream_predicate_builtins() {
    let program = parse_fragment(
        br#"echo stream_is_local("php://memory") ? "local" : "bad"; echo ":";
echo stream_supports_lock($handle) ? "lock" : "bad"; echo ":";
echo call_user_func("stream_is_local", "file://tmp") ? "call" : "bad"; echo ":";
echo call_user_func_array("stream_supports_lock", ["stream" => $handle]) ? "spread" : "bad"; echo ":";
echo function_exists("stream_is_local");
return function_exists("stream_supports_lock");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let handle = values.alloc(FakeValue::Resource(6));
    scope.set("handle", handle, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "local:lock:call:spread:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `spl_classes()` returns the shared catalog's `ext/spl` class list.
#[test]
fn execute_program_dispatches_spl_classes_builtin() {
    let program = parse_fragment(
        br#"$names = spl_classes();
echo count($names) . ":" . $names[0] . ":" . $names[53] . ":";
echo (in_array("LogicException", $names) ? "exception" : "bad") . ":";
echo (in_array("SplDoublyLinkedList", $names) ? "list" : "bad") . ":";
$call = call_user_func("spl_classes");
echo (in_array("SplStack", $call) ? "call" : "bad") . ":";
$spread = call_user_func_array("spl_classes", []);
echo (count($spread) === count($names) ? "spread" : "bad") . ":";
echo function_exists("spl_classes");
return is_callable("spl_classes");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "54:AppendIterator:UnexpectedValueException:exception:list:call:spread:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval SPL object identity builtins are stable, unique, and callable.
#[test]
fn execute_program_dispatches_spl_object_identity_builtins() {
    let program = parse_fragment(
            br#"$a = new KnownClass();
$b = new KnownClass();
echo (spl_object_id($a) === spl_object_id($a)) ? "stable" : "drift";
echo ":";
echo (spl_object_id($a) !== spl_object_id($b)) ? "unique" : "same";
echo ":";
echo (spl_object_hash(object: $a) === spl_object_hash($a)) ? "hash" : "bad";
echo ":";
echo (call_user_func("spl_object_id", $a) === spl_object_id($a)) ? "call" : "bad";
echo ":";
echo (call_user_func_array("spl_object_hash", ["object" => $b]) === spl_object_hash($b)) ? "array" : "bad";
echo ":";
echo function_exists("spl_object_id");
return function_exists("spl_object_hash");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "stable:unique:hash:call:array:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval environment builtins read, write, unset, and dispatch dynamically.
#[test]
fn execute_program_dispatches_environment_builtins() {
    let program = parse_fragment(
            br#"putenv("ELEPHC_EVAL_ENV_TEST=direct");
echo getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv(assignment: "ELEPHC_EVAL_ENV_TEST=named");
echo getenv(name: "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func("getenv", "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func_array("putenv", ["assignment" => "ELEPHC_EVAL_ENV_TEST=spread"]) ? "set" : "bad";
echo ":" . getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv("ELEPHC_EVAL_ENV_TEST");
echo getenv("ELEPHC_EVAL_ENV_TEST") === false ? "missing" : "bad";
echo ":"; echo function_exists("getenv");
return function_exists("putenv");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "direct:named:named:set:spread:missing:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `getenv()` with no name, a null name, and `local_only` answers the environment.
#[test]
fn execute_program_dispatches_getenv_whole_environment() {
    let program = parse_fragment(
        br#"putenv("ELEPHC_EVAL_ENV_ALL=present");
echo is_array(getenv()) ? "a" : "x";
echo is_array(getenv(null)) ? "n" : "x";
echo is_array(getenv(null, true)) ? "nt" : "x";
echo is_array(getenv(local_only: true)) ? "lo" : "x";
echo getenv("ELEPHC_EVAL_ENV_ALL", true);
return is_array(call_user_func("getenv"));"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "anntlopresent");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval shell process builtins capture or echo stdout across all call paths.
#[test]
fn execute_program_dispatches_process_builtins() {
    let program = parse_fragment(
        br#"echo shell_exec("printf shell"); echo ":";
echo exec(command: "printf exec"); echo ":";
echo system("printf system") === "" ? "empty" : "bad"; echo ":";
echo passthru(command: "printf pass") === null ? "null" : "bad"; echo ":";
echo call_user_func("shell_exec", "printf call"); echo ":";
echo call_user_func_array("exec", ["command" => "printf spread"]); echo ":";
echo call_user_func("system", "printf dynsys") === "" ? "dyn-empty" : "bad"; echo ":";
echo call_user_func_array("passthru", ["command" => "printf dynpass"]) === null ? "dyn-null" : "bad"; echo ":";
echo function_exists("exec"); echo function_exists("shell_exec"); echo function_exists("system");
return function_exists("passthru");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "shell:exec:systemempty:passnull:call:spread:dynsysdyn-empty:dynpassdyn-null:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval sleep builtins dispatch without delaying focused tests.
#[test]
fn execute_program_dispatches_sleep_builtins() {
    let program = parse_fragment(
        br#"echo sleep(0) . ":";
echo sleep(seconds: 0) . ":";
usleep(0);
echo "u:";
echo call_user_func("sleep", 0) . ":";
echo call_user_func_array("usleep", ["microseconds" => 0]) === null ? "null" : "bad";
echo ":"; echo function_exists("sleep");
return function_exists("usleep");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "0:0:u:0:null:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `php_uname()` dispatches default, named, mode, and callable calls.
#[test]
fn execute_program_dispatches_php_uname_builtin() {
    let program = parse_fragment(
        br#"echo strlen(php_uname()) > 0 ? "all" : "empty"; echo ":";
echo php_uname() === php_uname("a") ? "same" : "different"; echo ":";
echo strlen(php_uname(mode: "s")) > 0 ? "sys" : "empty"; echo ":";
echo strlen(php_uname("n")) > 0 ? "node" : "empty"; echo ":";
echo strlen(php_uname("r")) > 0 ? "release" : "empty"; echo ":";
echo strlen(php_uname("v")) > 0 ? "version" : "empty"; echo ":";
echo strlen(php_uname("m")) > 0 ? "machine" : "empty"; echo ":";
echo strlen(call_user_func("php_uname", "m")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("php_uname", ["mode" => "n"])) > 0 ? "spread" : "empty"; echo ":";
return function_exists("php_uname");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "all:same:sys:node:release:version:machine:call:spread:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `gethostbyname()` handles IPv4 literals and failed lookups.
#[test]
fn execute_program_dispatches_gethostbyname_builtin() {
    let program = parse_fragment(
        br#"echo gethostbyname("127.0.0.1") . ":";
echo gethostbyname(hostname: "not a host") . ":";
echo call_user_func("gethostbyname", "127.0.0.1") . ":";
echo call_user_func_array("gethostbyname", ["hostname" => "not a host"]) . ":";
return function_exists("gethostbyname");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "127.0.0.1:not a host:127.0.0.1:not a host:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `gethostname()` dispatches direct and callable zero-arg calls.
#[test]
fn execute_program_dispatches_gethostname_builtin() {
    let program = parse_fragment(
        br#"echo strlen(gethostname()) > 0 ? "host" : "empty"; echo ":";
echo strlen(call_user_func("gethostname")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("gethostname", [])) > 0 ? "spread" : "empty"; echo ":";
return function_exists("gethostname");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "host:call:spread:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `gethostbyaddr()` handles valid, malformed, and callable calls.
#[test]
fn execute_program_dispatches_gethostbyaddr_builtin() {
    let program = parse_fragment(
            br#"echo strlen(gethostbyaddr("127.0.0.1")) > 0 ? "direct" : "empty"; echo ":";
echo strlen(gethostbyaddr(ip: "127.0.0.1")) > 0 ? "named" : "empty"; echo ":";
echo gethostbyaddr("not-an-ip-address") === false ? "false" : "bad"; echo ":";
echo strlen(call_user_func("gethostbyaddr", "127.0.0.1")) > 0 ? "call" : "empty"; echo ":";
echo call_user_func_array("gethostbyaddr", ["ip" => "not-an-ip-address"]) === false ? "spread" : "bad"; echo ":";
return function_exists("gethostbyaddr");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "direct:named:false:call:spread:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval protocol and service database lookups dispatch dynamically.
#[test]
fn execute_program_dispatches_protocol_service_builtins() {
    let program = parse_fragment(
            br#"echo getprotobyname("TCP") . ":";
echo getprotobynumber(6) . ":";
echo getprotobyname("no_such_protocol") === false ? "missing-proto" : "bad"; echo ":";
echo getprotobynumber(999) === false ? "missing-number" : "bad"; echo ":";
echo getservbyname("www", "tcp") . ":";
echo getservbyport(80, "tcp") . ":";
echo getservbyname("no_such_service", "tcp") === false ? "missing-service" : "bad"; echo ":";
echo getservbyport(80, "no_such_proto") === false ? "missing-port" : "bad"; echo ":";
echo call_user_func("getprotobyname", "udp") . ":";
echo call_user_func_array("getprotobynumber", ["protocol" => 17]) . ":";
echo call_user_func("getservbyname", "https", "tcp") . ":";
echo call_user_func_array("getservbyport", ["port" => 443, "protocol" => "tcp"]) . ":";
echo function_exists("getprotobyname"); echo function_exists("getprotobynumber"); echo function_exists("getservbyname");
return function_exists("getservbyport");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "6:tcp:missing-proto:missing-number:80:http:missing-service:missing-port:17:udp:443:https:111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval IPv4 conversion builtins handle scalar and raw-byte paths.
#[test]
fn execute_program_dispatches_ip_conversion_builtins() {
    let program = parse_fragment(
        br#"echo long2ip(3232235777) . ":";
echo long2ip(ip: 4294967295) . ":";
echo ip2long("192.168.1.1") . ":";
echo ip2long(ip: "1.2.3") === false ? "bad-ip" : "bad"; echo ":";
$packed = inet_pton("1.2.3.4");
echo bin2hex($packed) . ":";
echo inet_pton(ip: "nonsense") === false ? "bad-pton" : "bad"; echo ":";
echo inet_ntop($packed) . ":";
echo inet_ntop(ip: "xx") === false ? "bad-ntop" : "bad"; echo ":";
echo call_user_func("long2ip", 2130706433) . ":";
echo call_user_func_array("ip2long", ["ip" => "0.0.0.0"]) . ":";
echo function_exists("long2ip"); echo function_exists("ip2long");
echo function_exists("inet_pton");
return function_exists("inet_ntop");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "192.168.1.1:255.255.255.255:3232235777:bad-ip:01020304:bad-pton:1.2.3.4:bad-ntop:127.0.0.1:0:111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `get_loaded_extensions()` returns the compile-time-known extension lists.
///
/// Curl is the one deliberate exception to these otherwise-static lists (mirrors
/// `extension_loaded('curl')` — see
/// `crate::interpreter::builtins::network_env::extension_loaded`'s module doc): with
/// `--features curl` it both appears in the list and satisfies `in_array`, so this test
/// reads the expected shape from `cfg!(feature = "curl")` rather than hard-coding one.
#[test]
fn execute_program_dispatches_get_loaded_extensions_builtin() {
    let curl_present = cfg!(feature = "curl");
    let source = format!(
        r#"$ext = get_loaded_extensions();
echo count($ext) . ":" . $ext[0] . ":";
echo (in_array("json", $ext) ? "json" : "bad") . ":";
echo (in_array("Zend OPcache", $ext) ? "opcache" : "bad") . ":";
echo (in_array("curl", $ext) ? "curl-present" : "curl-absent") . ":";
$zend = get_loaded_extensions(true);
echo count($zend) . ":" . $zend[0] . ":";
echo (in_array("Reflection", $zend) ? "bad" : "no-reflection") . ":";
echo is_array($ext) ? "array" : "bad";
return function_exists("get_loaded_extensions");"#,
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let count = if curl_present { 12 } else { 11 };
    let curl_label = if curl_present { "curl-present" } else { "curl-absent" };
    assert_eq!(
        values.output,
        format!("{count}:Core:json:opcache:{curl_label}:1:Zend OPcache:no-reflection:array")
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `get_extension_funcs()` preserves the date inventory and fallback contract.
#[test]
fn execute_program_dispatches_get_extension_funcs_builtin() {
    let program = parse_fragment(
        br#"$date = get_extension_funcs("DATE");
echo count($date) . ":" . $date[0] . ":" . $date[47] . ":";
echo get_extension_funcs("missing") === false ? "false" : "bad";
return function_exists("get_extension_funcs");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "48:strtotime:date_sun_info:false");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval applies php-src scalar binding and exposes invalid types as catchable errors.
#[test]
fn execute_program_get_extension_funcs_coercions_are_php_compatible() {
    let program = parse_fragment(
        br#"echo get_extension_funcs(0) === false ? "scalar" : "bad";
try {
    get_extension_funcs([]);
} catch (TypeError $error) {
    echo "|" . $error->getMessage();
}
return get_extension_funcs(null) === false;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "scalar|get_extension_funcs(): Argument #1 ($extension) must be of type string, array given"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(
        values.warnings,
        vec!["\nDeprecated: get_extension_funcs(): Passing null to parameter #1 ($extension) of type string is deprecated"]
    );
}

/// Verifies eval strict-types calls reject scalar coercion for get_extension_funcs().
#[test]
fn execute_program_get_extension_funcs_honors_strict_types() {
    let program = parse_fragment(
        br#"declare(strict_types=1);
try {
    get_extension_funcs(0);
    echo "bad";
} catch (TypeError $error) {
    echo $error->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "get_extension_funcs(): Argument #1 ($extension) must be of type string, int given"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert!(values.warnings.is_empty());
}

/// Verifies eval callables retain their defining compilation unit's strictness.
#[test]
fn execute_program_get_extension_funcs_retains_callable_lexical_strictness() {
    let strict_definitions = parse_fragment(
        br#"declare(strict_types=1);
function strict_eval_function() { return get_extension_funcs(0); }
$strict_eval_closure = static function() { return get_extension_funcs(0); };
class StrictEvalMethod { public static function direct() { return get_extension_funcs(0); } }
trait StrictEvalTrait { public function imported() { return get_extension_funcs(0); } }
class StrictEvalTraitConsumer { use StrictEvalTrait; }"#,
    )
    .expect("parse strict callable definitions");
    let weak_calls = parse_fragment(
        br#"try { strict_eval_function(); echo "function"; } catch (TypeError $error) { echo "F"; }
try { $strict_eval_closure(); echo "closure"; } catch (TypeError $error) { echo "C"; }
try { StrictEvalMethod::direct(); echo "method"; } catch (TypeError $error) { echo "M"; }
$trait_consumer = new StrictEvalTraitConsumer();
try { $trait_consumer->imported(); echo "trait"; } catch (TypeError $error) { echo "T"; }
return true;"#,
    )
    .expect("parse weak callable invocations");
    let weak_definitions = parse_fragment(
        br#"function weak_eval_function() { return get_extension_funcs(0) === false; }
$weak_eval_closure = static function() { return get_extension_funcs(0) === false; };
class WeakEvalMethod { public static function direct() { return get_extension_funcs(0) === false; } }
trait WeakEvalTrait { public function imported() { return get_extension_funcs(0) === false; } }
class WeakEvalTraitConsumer { use WeakEvalTrait; }"#,
    )
    .expect("parse weak callable definitions");
    let strict_calls = parse_fragment(
        br#"declare(strict_types=1);
$trait_consumer = new WeakEvalTraitConsumer();
return weak_eval_function()
    && $weak_eval_closure()
    && WeakEvalMethod::direct()
    && $trait_consumer->imported();"#,
    )
    .expect("parse strict callable invocations");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program_with_context(
        &mut context,
        &strict_definitions,
        &mut scope,
        &mut values,
    )
    .expect("declare strict callables");
    let weak_result = execute_program_with_context(&mut context, &weak_calls, &mut scope, &mut values)
        .expect("weak caller must catch each strict callable error");

    assert_eq!(values.output, "FCMT");
    assert_eq!(values.get(weak_result), FakeValue::Bool(true));

    execute_program_with_context(&mut context, &weak_definitions, &mut scope, &mut values)
        .expect("declare weak callables");
    let strict_result =
        execute_program_with_context(&mut context, &strict_calls, &mut scope, &mut values)
            .expect("strict caller must not alter weak callable bodies");

    assert_eq!(values.get(strict_result), FakeValue::Bool(true));
    assert!(values.warnings.is_empty());
}

/// Verifies eval `extension_loaded()` resolves the compile-time-known extension set.
///
/// `curl` is the one deliberate exception that tracks `cfg!(feature = "curl")` instead of
/// a fixed answer — see
/// `crate::interpreter::builtins::network_env::extension_loaded`'s module doc — so the
/// expected trailing digit is read from that same `cfg!` rather than hard-coded.
#[test]
fn execute_program_dispatches_extension_loaded_builtin() {
    let program = parse_fragment(
        br#"echo extension_loaded("json") ? "1" : "0";
echo extension_loaded("Zend OPcache") ? "1" : "0";
echo extension_loaded("opcache") ? "1" : "0";
echo extension_loaded("curl") ? "1" : "0";
return function_exists("extension_loaded");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let curl_digit = if cfg!(feature = "curl") { "1" } else { "0" };
    assert_eq!(values.output, format!("110{curl_digit}"));
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// REGRESSION (CI flake, `curl-feature-contract` job): a shell command that could not be
/// SPAWNED must not be reported as a command that ran and printed nothing.
///
/// The runner used to end in `.output().map(|o| o.stdout).unwrap_or_default()`, collapsing
/// the two. On a loaded runner `posix_spawn` fails with `EAGAIN`, so one `exec()` in an
/// otherwise-passing test silently answered `""` — measured here directly: 2560 concurrent
/// `/bin/sh -c "printf spread"` spawns under `ulimit -u 900` produced 1177 `EAGAIN`s, every
/// one of which the old code called "printed nothing". `false` is php-src's own failure
/// value (`php_exec` returning `FAILURE` ends in `RETURN_FALSE`).
///
/// Driven through `eval_process_outcome_result` rather than by inducing a real spawn
/// failure, so the test is DETERMINISTIC: reproducing the failure for real needs
/// process-table pressure, which is precisely the nondeterminism a regression test must not
/// depend on.
#[test]
fn a_failed_spawn_reports_false_rather_than_an_empty_command() {
    for name in ["exec", "shell_exec", "system", "passthru"] {
        let mut values = FakeOps::default();
        let result = eval_process_outcome_result(name, EvalShellOutcome::SpawnFailed, &mut values)
            .expect("spawn failure must be a value, not a fatal");
        assert_eq!(
            values.get(result),
            FakeValue::Bool(false),
            "{name}() must report a failed spawn as false"
        );
        assert_eq!(
            values.output, "",
            "{name}() must not echo anything for a command that never ran"
        );
    }
}

/// NEGATIVE CONTROL for the test above, and the one that makes it meaningful: a command that
/// genuinely RAN and printed nothing keeps every answer it always had. Without this, a fix
/// that simply reported `false` whenever the output was empty would pass the regression test
/// while breaking `system()`/`passthru()`, whose normal successful return is exactly the
/// empty-output shape.
#[test]
fn a_successful_command_with_no_output_keeps_its_php_return_value() {
    // All three string answers land as `String("")` in the fake — `string_bytes_value` on
    // empty, UTF-8-clean bytes and `string("")` converge — while `passthru` is PHP `null`.
    let cases: [(&str, FakeValue); 4] = [
        ("exec", FakeValue::String(String::new())),
        ("shell_exec", FakeValue::String(String::new())),
        ("system", FakeValue::String(String::new())),
        ("passthru", FakeValue::Null),
    ];
    for (name, expected) in cases {
        let mut values = FakeOps::default();
        let result =
            eval_process_outcome_result(name, EvalShellOutcome::Ran(Vec::new()), &mut values)
                .expect("a successful empty command must be a value");
        assert_eq!(
            values.get(result),
            expected,
            "{name}() must keep its documented return for a command that printed nothing"
        );
        assert_eq!(values.output, "", "{name}() must echo nothing for empty output");
    }
}

/// The same regression as `a_failed_spawn_reports_false_rather_than_an_empty_command`, but
/// driven END TO END through real PHP source and a REAL spawn failure, so it exercises the
/// runner itself rather than only the outcome mapping.
///
/// The failure is induced deterministically with an over-long argument: `posix_spawn` rejects
/// it with `E2BIG` (`ArgumentListTooLong`, raw os error 7) every time, on every machine, with
/// no process-table pressure and no timing involved. `E2BIG` is a standing condition, so the
/// runner classifies it as non-transient and fails immediately without burning the retry
/// budget — which this test also pins by construction, since a retried command would still
/// end in the same answer but take five sleeps to get there.
///
/// BEFORE THE FIX this asserted `""`: the runner's `.unwrap_or_default()` reported a command
/// that could not be created as one that ran and printed nothing.
#[test]
fn a_command_that_cannot_be_spawned_is_false_not_an_empty_string() {
    // Comfortably past `ARG_MAX` on every supported platform.
    let oversized = "x".repeat(4 * 1024 * 1024);
    let source = format!("return exec(\"printf {oversized}\");");
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::Bool(false),
        "a command that could not be spawned must be false, never an empty string"
    );
}
