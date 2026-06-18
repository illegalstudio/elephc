//! Purpose:
//! Interpreter tests for time, system, SPL, environment, host, protocol, and IP builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases isolate platform-facing builtins behind deterministic fake assertions where possible.

use super::super::*;
use super::support::*;

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
            eval_compiler_php_version(),
            eval_compiler_php_version()
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
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
echo ":"; echo function_exists("date");
return function_exists("mktime");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "2024-01-02 13:02:03:2-1-13-1-PM-pm-2-Tue-Jan-Tuesday-January:U:2024:2000:1"
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
echo ":" . (strtotime("2024/06/15") === -1 ? "bad" : "wrong");
$call = call_user_func("strtotime", "2024-01-02 03:04:05");
echo ":" . date("Y-m-d H:i:s", $call);
$spread = call_user_func_array("strtotime", ["datetime" => "2024-01-02"]);
echo ":" . date("Y-m-d", $spread) . ":";
return function_exists("strtotime");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "2024-06-15 00:00:00:2024-06-15 12:30:45:2024-06-15 12:30:00:bad:2024-01-02 03:04:05:2024-01-02:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `microtime()` returns a plausible float timestamp by all call paths.
#[test]
fn execute_program_dispatches_microtime_builtin() {
    let program = parse_fragment(
        br#"echo microtime() > 1000000000 ? "now" : "bad"; echo ":";
echo microtime(as_float: false) > 1000000000 ? "named" : "bad"; echo ":";
echo call_user_func("microtime", true) > 1000000000 ? "call" : "bad"; echo ":";
echo call_user_func_array("microtime", ["as_float" => true]) > 1000000000 ? "array" : "bad";
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
/// Verifies eval `spl_classes()` returns the native-compatible SPL type snapshot.
#[test]
fn execute_program_dispatches_spl_classes_builtin() {
    let program = parse_fragment(
        br#"$names = spl_classes();
echo count($names) . ":" . $names[0] . ":" . $names[55] . ":";
echo (in_array("Exception", $names) ? "exception" : "bad") . ":";
echo (in_array("SplDoublyLinkedList", $names) ? "list" : "bad") . ":";
$call = call_user_func("spl_classes");
echo (in_array("Throwable", $call) ? "call" : "bad") . ":";
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
        "61:AppendIterator:Throwable:exception:list:call:spread:1"
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
echo getenv("ELEPHC_EVAL_ENV_TEST") === "" ? "empty" : "bad";
echo ":"; echo function_exists("getenv");
return function_exists("putenv");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "direct:named:named:set:spread:empty:1");
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
