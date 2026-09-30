//! Purpose:
//! Interpreter tests for eval local file stream resource builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases use process-unique local files and clean them before and after execution.
//! - Stream resources are eval-owned fake/runtime cells whose ids map into context state.

use super::super::*;
use super::support::*;

/// Verifies eval file stream resources support open/read/write/seek/stat/close operations.
#[test]
fn execute_program_dispatches_file_stream_builtins() {
    let pid = std::process::id();
    let file = format!("elephc_magician_stream_file_{pid}.txt");
    let copy = format!("elephc_magician_stream_copy_{pid}.txt");
    let call = format!("elephc_magician_stream_call_{pid}.txt");
    let source = format!(
        r#"$h = fopen(filename: "{file}", mode: "w+");
echo is_resource($h) ? "open" : "bad"; echo ":";
echo get_resource_type($h) === "stream" ? "rtype" : "bad"; echo ":";
echo get_resource_id($h) >= 1 ? "rid" : "bad"; echo ":";
echo fwrite($h, "abcdef") === 6 ? "write" : "bad"; echo ":";
echo ftell($h) === 6 ? "tell" : "bad"; echo ":";
echo rewind($h) ? "rewind" : "bad"; echo ":";
echo fread($h, 2) === "ab" ? "read" : "bad"; echo ":";
echo fseek($h, 1) === 0 ? "seek" : "bad"; echo ":";
echo stream_get_contents($h, 3) === "bcd" ? "bounded" : "bad"; echo ":";
rewind($h);
echo stream_get_contents($h) === "abcdef" ? "contents" : "bad"; echo ":";
echo feof($h) ? "eof" : "bad"; echo ":";
$meta = stream_get_meta_data($h);
echo $meta["wrapper_type"] === "plainfile" && $meta["stream_type"] === "STDIO" && $meta["mode"] === "w+" ? "meta" : "bad"; echo ":";
echo ftruncate($h, 3) ? "truncate" : "bad"; echo ":";
$stat = fstat($h);
echo $stat["size"] === 3 ? "fstat" : "bad"; echo ":";
echo fflush($h) && fsync($h) && fdatasync($h) ? "sync" : "bad"; echo ":";
echo fclose($h) ? "close" : "bad"; echo ":";
echo file_get_contents("{file}") === "abc" ? "truncated" : "bad"; echo ":";
$src = fopen("{file}", "r");
$dst = fopen("{copy}", "w+");
echo stream_copy_to_stream($src, $dst, null, 1) === 2 ? "copy" : "bad"; echo ":";
rewind($dst);
echo stream_get_contents($dst) === "bc" ? "copied" : "bad"; echo ":";
fclose($src);
fclose($dst);
$tmp = tmpfile();
echo is_resource($tmp) ? "tmp" : "bad"; echo ":";
fwrite($tmp, "xy");
rewind($tmp);
echo fread($tmp, 2) === "xy" ? "tmpread" : "bad"; echo ":";
fclose($tmp);
$call = call_user_func_array("fopen", ["filename" => "{call}", "mode" => "w+"]);
echo call_user_func_array("fwrite", ["stream" => $call, "data" => "zz"]) === 2 ? "callwrite" : "bad"; echo ":";
call_user_func("rewind", $call);
echo call_user_func("fread", $call, 2) === "zz" ? "callread" : "bad"; echo ":";
echo call_user_func("fclose", $call) ? "callclose" : "bad"; echo ":";
echo unlink("{file}") && unlink("{copy}") && unlink("{call}") ? "cleanup" : "bad"; echo ":";
echo function_exists("fopen"); echo function_exists("fclose"); echo function_exists("fread");
echo function_exists("fwrite"); echo function_exists("feof"); echo function_exists("fflush");
echo function_exists("ftell"); echo function_exists("fseek"); echo function_exists("rewind");
echo function_exists("ftruncate"); echo function_exists("fsync"); echo function_exists("fdatasync");
echo function_exists("fstat"); echo function_exists("stream_get_contents");
echo function_exists("stream_copy_to_stream"); echo function_exists("stream_get_meta_data");
echo function_exists("tmpfile");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    for path in [&file, &copy, &call] {
        let _ = std::fs::remove_file(path);
    }
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    for path in [&file, &copy, &call] {
        let _ = std::fs::remove_file(path);
    }
    assert_eq!(
        values.output,
        "open:rtype:rid:write:tell:rewind:read:seek:bounded:contents:eof:meta:truncate:fstat:sync:close:truncated:copy:copied:tmp:tmpread:callwrite:callread:callclose:cleanup:11111111111111111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `fstat()` returns PHP's complete numeric and string-keyed array.
#[test]
fn execute_program_dispatches_complete_fstat_array() {
    let pid = std::process::id();
    let file = format!("elephc_magician_complete_fstat_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "abcdef");
$h = fopen("{file}", "r");
$stat = fstat($h);
echo count($stat) === 26 ? "count" : "bad"; echo ":";
echo $stat[0] === $stat["dev"] && $stat[1] === $stat["ino"] && $stat[2] === $stat["mode"] ? "head" : "bad"; echo ":";
echo $stat[3] === $stat["nlink"] && $stat[4] === $stat["uid"] && $stat[5] === $stat["gid"] ? "owner" : "bad"; echo ":";
echo $stat[6] === $stat["rdev"] && $stat[7] === $stat["size"] && $stat["size"] === 6 ? "size" : "bad"; echo ":";
echo $stat[8] === $stat["atime"] && $stat[9] === $stat["mtime"] && $stat[10] === $stat["ctime"] ? "time" : "bad"; echo ":";
echo $stat[11] === $stat["blksize"] && $stat[12] === $stat["blocks"] ? "blocks" : "bad"; echo ":";
fclose($h);
echo unlink("{file}") ? "cleanup" : "bad";
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&file);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&file);
    assert_eq!(
        values.output,
        "count:head:owner:size:time:blocks:cleanup"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `flock()` applies advisory locks to local file stream resources.
#[test]
fn execute_program_dispatches_file_stream_flock_builtin() {
    let pid = std::process::id();
    let file = format!("elephc_magician_stream_lock_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "x");
$h = fopen("{file}", "r+");
$would = true;
echo flock($h, LOCK_EX, $would) ? "lock" : "bad"; echo ":";
echo $would === false ? "would0" : "bad"; echo ":";
echo flock(stream: $h, operation: LOCK_UN, would_block: $would) ? "unlock" : "bad"; echo ":";
echo $would === false ? "would1" : "bad"; echo ":";
class EvalFlockWouldBlockBox {{
    public bool $value = true;
    public static bool $staticValue = true;
}}
$box = new EvalFlockWouldBlockBox();
echo flock($h, LOCK_SH, $box->value) ? "proplock" : "bad"; echo ":";
echo $box->value === false ? "prop0" : "bad"; echo ":";
flock($h, LOCK_UN);
$name = "value";
echo flock($h, LOCK_EX, $box->{{$name}}) ? "dynlock" : "bad"; echo ":";
echo $box->value === false ? "dyn0" : "bad"; echo ":";
flock($h, LOCK_UN);
echo flock($h, LOCK_SH, EvalFlockWouldBlockBox::$staticValue) ? "staticlock" : "bad"; echo ":";
echo EvalFlockWouldBlockBox::$staticValue === false ? "static0" : "bad"; echo ":";
flock($h, LOCK_UN);
$class = "EvalFlockWouldBlockBox";
$staticName = "staticValue";
echo flock($h, LOCK_EX, $class::${{$staticName}}) ? "dynstaticlock" : "bad"; echo ":";
echo EvalFlockWouldBlockBox::$staticValue === false ? "dynstatic0" : "bad"; echo ":";
flock($h, LOCK_UN);
$lock = "flock";
$dynamicWould = true;
echo $lock($h, LOCK_SH, $dynamicWould) ? "dynfnlock" : "bad"; echo ":";
echo $dynamicWould === false ? "dynfn0" : "bad"; echo ":";
flock($h, LOCK_UN);
$firstClassLock = flock(...);
$firstClassWould = true;
echo $firstClassLock($h, LOCK_EX, $firstClassWould) ? "fcclock" : "bad"; echo ":";
echo $firstClassWould === false ? "fcc0" : "bad"; echo ":";
flock($h, LOCK_UN);
echo call_user_func("flock", $h, LOCK_SH) ? "calllock" : "bad"; echo ":";
flock($h, LOCK_UN);
echo flock($h, 99) === false ? "invalid" : "bad"; echo ":";
fclose($h);
echo unlink("{file}") ? "cleanup" : "bad"; echo ":";
echo function_exists("flock");
echo defined("LOCK_SH"); echo defined("LOCK_EX"); echo defined("LOCK_UN"); echo defined("LOCK_NB");
echo ":locks=" . LOCK_SH . LOCK_EX . LOCK_UN . LOCK_NB;
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&file);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&file);
    assert_eq!(
        values.output,
        "lock:would0:unlock:would1:proplock:prop0:dynlock:dyn0:staticlock:static0:dynstaticlock:dynstatic0:dynfnlock:dynfn0:fcclock:fcc0:calllock:invalid:cleanup:11111:locks=1234"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval line, character, and passthrough stream builtins.
#[test]
fn execute_program_dispatches_file_stream_line_builtins() {
    let pid = std::process::id();
    let file = format!("elephc_magician_stream_lines_{pid}.txt");
    let ending = format!("elephc_magician_stream_ending_{pid}.txt");
    let formatted = format!("elephc_magician_stream_formatted_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "a\nbc\nxyz");
$h = fopen("{file}", "r");
echo fgetc($h) === "a" ? "char" : "bad"; echo ":";
echo fgets($h) === "\n" ? "line1" : "bad"; echo ":";
echo fgets($h) === "bc\n" ? "line2" : "bad"; echo ":";
echo stream_get_line($h, 2) === "xy" ? "getline" : "bad"; echo ":";
echo "[";
$passed = fpassthru($h);
echo "]";
echo $passed === 1 ? "passthru" : "bad"; echo ":";
echo feof($h) ? "eof" : "bad"; echo ":";
fclose($h);
file_put_contents("{ending}", "leftENDright");
$e = fopen("{ending}", "r");
echo stream_get_line($e, 20, "END") === "left" ? "ending" : "bad"; echo ":";
echo stream_get_contents($e) === "right" ? "after" : "bad"; echo ":";
fclose($e);
$call = call_user_func("fopen", "{file}", "r");
echo call_user_func("fgetc", $call) === "a" ? "callchar" : "bad"; echo ":";
echo call_user_func_array("stream_get_line", ["stream" => $call, "length" => 2]) === "\nb" ? "callline" : "bad"; echo ":";
call_user_func("fclose", $call);
$fmt = fopen("{formatted}", "w+");
echo fprintf($fmt, "%s-%d", "n", 7) === 3 ? "fprintf" : "bad"; echo ":";
echo vfprintf($fmt, "-%s", ["x"]) === 2 ? "vfprintf" : "bad"; echo ":";
rewind($fmt);
echo stream_get_contents($fmt) === "n-7-x" ? "formatted" : "bad"; echo ":";
echo call_user_func("fprintf", $fmt, "-%s", "c") === 2 ? "callfprintf" : "bad"; echo ":";
echo call_user_func_array("vfprintf", ["stream" => $fmt, "format" => "-%d", "values" => [5]]) === 2 ? "callvfprintf" : "bad"; echo ":";
fclose($fmt);
echo unlink("{file}") && unlink("{ending}") && unlink("{formatted}") ? "cleanup" : "bad"; echo ":";
echo function_exists("fgetc"); echo function_exists("fgets"); echo function_exists("fpassthru");
echo function_exists("fprintf"); echo function_exists("stream_get_line"); echo function_exists("vfprintf");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    for path in [&file, &ending, &formatted] {
        let _ = std::fs::remove_file(path);
    }
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    for path in [&file, &ending, &formatted] {
        let _ = std::fs::remove_file(path);
    }
    assert_eq!(
        values.output,
        "char:line1:line2:getline:[z]passthru:eof:ending:after:callchar:callline:fprintf:vfprintf:formatted:callfprintf:callvfprintf:cleanup:111111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval CSV stream builtins format and parse local stream records.
///
/// `fscanf()`'s `%d` is compared to the INT `42`, not the string `"42"`: the strict
/// comparison used to read `=== "42"` and passed only because the old scanf subset
/// answered with the matched text. `php -n` (8.5.6) answers `int(42)` there, so the old
/// assertion was pinning a divergence rather than catching it.
#[test]
fn execute_program_dispatches_file_stream_csv_builtins() {
    let pid = std::process::id();
    let file = format!("elephc_magician_stream_csv_{pid}.txt");
    let semi = format!("elephc_magician_stream_csv_semi_{pid}.txt");
    let call = format!("elephc_magician_stream_csv_call_{pid}.txt");
    let scan = format!("elephc_magician_stream_scan_{pid}.txt");
    let source = format!(
        r#"$h = fopen("{file}", "w+");
echo fputcsv($h, ["a", "b,c", "d\"e"]) > 0 ? "put" : "bad"; echo ":";
rewind($h);
$row = fgetcsv($h);
echo $row[0] === "a" && $row[1] === "b,c" && $row[2] === "d\"e" ? "get" : "bad"; echo ":";
echo fgetcsv($h) === false ? "eof" : "bad"; echo ":";
fclose($h);
$semi = fopen("{semi}", "w+");
echo fputcsv($semi, ["x;y", "z"], ";") > 0 ? "putsemi" : "bad"; echo ":";
rewind($semi);
$semi_row = fgetcsv($semi, null, ";");
echo $semi_row[0] === "x;y" && $semi_row[1] === "z" ? "getsemi" : "bad"; echo ":";
fclose($semi);
$call = fopen("{call}", "w+");
echo call_user_func_array("fputcsv", ["stream" => $call, "fields" => ["m", "n"]]) > 0 ? "callput" : "bad"; echo ":";
rewind($call);
$call_row = call_user_func("fgetcsv", $call);
echo $call_row[0] === "m" && $call_row[1] === "n" ? "callget" : "bad"; echo ":";
fclose($call);
file_put_contents("{scan}", "42 alpha\n7 beta\n");
$scan = fopen("{scan}", "r");
$matched = fscanf($scan, "%d %s");
echo $matched[0] === 42 && $matched[1] === "alpha" ? "scan" : "bad"; echo ":";
$call_matched = call_user_func("fscanf", $scan, "%d %s");
echo $call_matched[0] === 7 && $call_matched[1] === "beta" ? "callscan" : "bad"; echo ":";
fclose($scan);
echo unlink("{file}") && unlink("{semi}") && unlink("{call}") && unlink("{scan}") ? "cleanup" : "bad"; echo ":";
echo function_exists("fgetcsv"); echo function_exists("fputcsv"); echo function_exists("fscanf");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    for path in [&file, &semi, &call, &scan] {
        let _ = std::fs::remove_file(path);
    }
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    for path in [&file, &semi, &call, &scan] {
        let _ = std::fs::remove_file(path);
    }
    assert_eq!(
        values.output,
        "put:get:eof:putsemi:getsemi:callput:callget:scan:callscan:cleanup:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `get_resource_id()` numbers eval streams 5, 6, 7 like PHP 8.5.6 does.
///
/// This is the unit-level guard for the eval payload namespace, and it runs the REAL
/// `EvalStreamResources` allocator against the fake mirror of the runtime resource-id
/// registry, so it fails if either half drifts. Without the namespace base the eval
/// payloads are 0, 1 and 2, which the registry answers as the standard streams'
/// fixed ids, and this program prints `1,2,3`.
///
/// `var_dump()` is asserted alongside because it read a hard-coded `resource(0)` for
/// as long as nothing checked it.
#[test]
fn execute_program_numbers_eval_stream_resources_like_php() {
    let program = parse_fragment(
        br#"$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
$c = fopen("php://memory", "r");
echo get_resource_id($a) . "," . get_resource_id($b) . "," . get_resource_id($c) . ":";
var_dump($a);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "5,6,7:resource(5) of type (stream)\n"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a closed eval stream burns its id instead of releasing it for reuse.
///
/// php-src draws `zend_resource` handles from a monotonically increasing list index,
/// so the stream opened after an `fclose()` takes the NEXT id: PHP 8.5.6 prints `5,7`
/// for the equivalent program, never `5,6`.
#[test]
fn execute_program_does_not_recycle_a_closed_eval_stream_id() {
    let program = parse_fragment(
        br#"$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
fclose($b);
$c = fopen("php://memory", "r");
echo get_resource_id($a) . "," . get_resource_id($c);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5,7");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a CLOSED eval-created stream renames its type to `Unknown`, in both
/// `var_dump()` and `get_resource_type()`.
///
/// Captured from PHP 8.5.6 running the equivalent program with `php -d xdebug.mode=off`.
/// An eval-created resource carries NO close sentinel — its payload is the key of
/// `EvalStreamResources` and negating it would break every builtin that later resolves
/// the handle — so the close state is read from the tables through
/// `EvalStreamResources::is_live`. `fclose()` removes the entry and `take_next_id()`
/// never reuses a payload, which is what makes "in no table" mean "closed".
///
/// The id must stay 5: php-src leaves `zend_resource.handle` alone on close, and only the
/// NAME changes.
#[test]
fn execute_program_renames_a_closed_eval_stream_to_unknown() {
    let program = parse_fragment(
        br#"$a = fopen("php://memory", "r");
var_dump($a);
echo get_resource_type($a) . ":";
fclose($a);
var_dump($a);
echo get_resource_type($a) . ":" . get_resource_id($a);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "resource(5) of type (stream)\nstream:resource(5) of type (Unknown)\nUnknown:5"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a HOST resource closed by the compiled program renames to `Unknown` too.
///
/// This is the OTHER close representation: a host `fclose`/`pclose`/`closedir` stamps the
/// `-id` sentinel into the Mixed box, so the cell arrives inside `eval()` carrying a
/// NEGATIVE payload rather than a missing table entry. `FakeValue::Resource(-5)` models
/// exactly that box.
///
/// The payload must be read as `as i64`, not through `i64::try_from`: the sentinel for id
/// 5 is `0xFFFF_FFFF_FFFF_FFFB` and `try_from` rejects it outright. The id stays 5, again
/// because php-src does not disturb the handle on close.
#[test]
fn execute_program_renames_a_closed_host_resource_to_unknown() {
    let program = parse_fragment(
        br#"var_dump($handle);
echo get_resource_type($handle) . ":" . get_resource_id($handle);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let handle = values.alloc(FakeValue::Resource(-5));
    scope.set("handle".to_string(), handle, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "resource(5) of type (Unknown)\nUnknown:5"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies an OPEN host resource still reports `stream`, so the closed test above cannot
/// pass by renaming everything.
///
/// A host payload is a plain file descriptor with no sentinel and no `EvalStreamResources`
/// entry, so it must fall through both close checks. This is the negative control for
/// `execute_program_renames_a_closed_host_resource_to_unknown`: without it, a predicate
/// that answered "closed" for every host payload would look correct.
#[test]
fn execute_program_keeps_an_open_host_resource_named_stream() {
    let program = parse_fragment(
        br#"var_dump($handle);
echo get_resource_type($handle);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let handle = values.alloc(FakeValue::Resource(6));
    scope.set("handle".to_string(), handle, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "resource(5) of type (stream)\nstream");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
