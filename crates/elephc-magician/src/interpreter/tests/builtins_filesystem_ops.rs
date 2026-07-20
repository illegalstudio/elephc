//! Purpose:
//! Interpreter tests for filesystem listing and mutating builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover directory traversal, globbing, file changes, and touch behavior.

use super::super::*;
use super::support::*;

/// Verifies eval local path operation builtins mutate filesystem state.
#[test]
fn execute_program_dispatches_path_operation_builtins() {
    let pid = std::process::id();
    let dir = format!("elephc_magician_ops_dir_{pid}");
    let call_dir = format!("elephc_magician_ops_call_dir_{pid}");
    let src = format!("elephc_magician_ops_src_{pid}.txt");
    let copy = format!("elephc_magician_ops_copy_{pid}.txt");
    let moved = format!("elephc_magician_ops_moved_{pid}.txt");
    let symlink = format!("elephc_magician_ops_symlink_{pid}.txt");
    let hardlink = format!("elephc_magician_ops_hardlink_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{src}", "hello");
echo mkdir("{dir}") ? "mkdir" : "bad"; echo ":";
echo is_dir("{dir}") ? "dir" : "bad"; echo ":";
echo copy("{src}", "{copy}") && file_get_contents("{copy}") === "hello" ? "copy" : "bad"; echo ":";
echo rename("{copy}", "{moved}") && file_exists("{moved}") && !file_exists("{copy}") ? "rename" : "bad"; echo ":";
echo symlink("{src}", "{symlink}") ? "symlink" : "bad"; echo ":";
echo readlink("{symlink}") === "{src}" ? "readlink" : "bad"; echo ":";
echo linkinfo("{symlink}") >= 0 ? "linkinfo" : "bad"; echo ":";
echo readlink("{src}") === false ? "readlink-false" : "bad"; echo ":";
echo linkinfo("{missing}") === -1 ? "linkinfo-missing" : "bad"; echo ":";
echo link("{src}", "{hardlink}") && file_get_contents("{hardlink}") === "hello" ? "hardlink" : "bad"; echo ":";
echo clearstatcache() === null ? "cache" : "bad"; echo ":";
echo unlink("{symlink}") && unlink("{hardlink}") && unlink("{moved}") && unlink("{src}") && rmdir("{dir}") ? "cleanup" : "bad"; echo ":";
echo call_user_func("mkdir", "{call_dir}") ? "callmkdir" : "bad"; echo ":";
echo call_user_func_array("rmdir", ["directory" => "{call_dir}"]) ? "callrmdir" : "bad"; echo ":";
echo function_exists("mkdir"); echo function_exists("rmdir"); echo function_exists("copy");
echo function_exists("rename"); echo function_exists("symlink"); echo function_exists("link");
echo function_exists("readlink"); echo function_exists("linkinfo"); echo function_exists("clearstatcache");
return true;"#,
        missing = format!("elephc_magician_ops_missing_{pid}.txt"),
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    for path in [&symlink, &hardlink, &moved, &copy, &src] {
        let _ = std::fs::remove_file(path);
    }
    for path in [&call_dir, &dir] {
        let _ = std::fs::remove_dir(path);
    }
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    for path in [&symlink, &hardlink, &moved, &copy, &src] {
        let _ = std::fs::remove_file(path);
    }
    for path in [&call_dir, &dir] {
        let _ = std::fs::remove_dir(path);
    }
    assert_eq!(
            values.output,
            "mkdir:dir:copy:rename:symlink:readlink:linkinfo:readlink-false:linkinfo-missing:hardlink:cache:cleanup:callmkdir:callrmdir:111111111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `stream_resolve_include_path()` mirrors elephc realpath semantics.
#[test]
fn execute_program_dispatches_stream_resolve_include_path_builtin() {
    let pid = std::process::id();
    let file = format!("elephc_magician_stream_resolve_{pid}.txt");
    let missing = format!("elephc_magician_stream_resolve_missing_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "payload");
$resolved = stream_resolve_include_path("{file}");
echo is_string($resolved) && basename($resolved) === "{file}" && file_get_contents($resolved) === "payload" ? "resolved" : "bad"; echo ":";
echo stream_resolve_include_path("{missing}") === false ? "missing" : "bad"; echo ":";
$named = stream_resolve_include_path(filename: "{file}");
echo is_string($named) && basename($named) === "{file}" ? "named" : "bad"; echo ":";
$call = call_user_func("stream_resolve_include_path", "{file}");
echo is_string($call) && basename($call) === "{file}" ? "call" : "bad"; echo ":";
$spread = call_user_func_array("stream_resolve_include_path", ["filename" => "{file}"]);
echo is_string($spread) && basename($spread) === "{file}" ? "spread" : "bad"; echo ":";
echo unlink("{file}") ? "cleanup" : "bad"; echo ":";
return function_exists("stream_resolve_include_path");"#,
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(&missing);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&file);
    assert_eq!(
        values.output,
        "resolved:missing:named:call:spread:cleanup:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval file-listing builtins build arrays, stream files, and dispatch dynamically.
#[test]
fn execute_program_dispatches_file_listing_builtins() {
    let pid = std::process::id();
    let lines = format!("elephc_magician_listing_lines_{pid}.txt");
    let empty = format!("elephc_magician_listing_empty_{pid}.txt");
    let missing = format!("elephc_magician_listing_missing_{pid}.txt");
    let dir = format!("elephc_magician_listing_dir_{pid}");
    let source = format!(
        r#"file_put_contents("{lines}", "one\ntwo");
file_put_contents("{empty}", "");
$lines = file("{lines}");
echo count($lines) . ":";
echo $lines[0] === "one\n" ? "line0" : "bad"; echo ":";
echo $lines[1] === "two" ? "line1" : "bad"; echo ":";
echo "[";
$bytes = readfile(filename: "{empty}");
echo "]" . $bytes . ":";
echo readfile("{missing}") === false ? "missing-readfile" : "bad"; echo ":";
echo count(file("{missing}")) === 0 ? "missing-file" : "bad"; echo ":";
mkdir("{dir}");
file_put_contents("{dir}/a.txt", "a");
file_put_contents("{dir}/b.txt", "b");
$scan = scandir(directory: "{dir}");
echo count($scan) . ":";
echo in_array(".", $scan) && in_array("..", $scan) && in_array("a.txt", $scan) && in_array("b.txt", $scan) ? "scan" : "bad"; echo ":";
$call_lines = call_user_func("file", "{lines}");
echo $call_lines[0] === "one\n" ? "callfile" : "bad"; echo ":";
$call_scan = call_user_func_array("scandir", ["directory" => "{dir}"]);
echo count($call_scan) . ":";
echo unlink("{dir}/a.txt") && unlink("{dir}/b.txt") && rmdir("{dir}") && unlink("{lines}") && unlink("{empty}") ? "cleanup" : "bad"; echo ":";
echo function_exists("file"); echo function_exists("readfile"); echo function_exists("scandir");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    for path in [&lines, &empty, &missing] {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_file(format!("{dir}/a.txt"));
    let _ = std::fs::remove_file(format!("{dir}/b.txt"));
    let _ = std::fs::remove_dir(&dir);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    for path in [&lines, &empty, &missing] {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_file(format!("{dir}/a.txt"));
    let _ = std::fs::remove_file(format!("{dir}/b.txt"));
    let _ = std::fs::remove_dir(&dir);
    assert_eq!(
        values.output,
        "2:line0:line1:[]0:missing-readfile:missing-file:4:scan:callfile:4:cleanup:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `glob()` expands local patterns and dispatches dynamically.
#[test]
fn execute_program_dispatches_glob_builtin() {
    let pid = std::process::id();
    let dir = format!("elephc_magician_glob_dir_{pid}");
    let source = format!(
        r#"mkdir("{dir}");
file_put_contents("{dir}/a.txt", "a");
file_put_contents("{dir}/b.log", "b");
file_put_contents("{dir}/c.txt", "c");
file_put_contents("{dir}/.hidden.txt", "h");
$matches = glob("{dir}/*.txt");
echo count($matches) === 2 && basename($matches[0]) === "a.txt" && basename($matches[1]) === "c.txt" ? "glob" : "bad"; echo ":";
echo count(glob("{dir}/*.none")) === 0 ? "empty" : "bad"; echo ":";
$literal = glob("{dir}/a.txt");
echo count($literal) === 1 && $literal[0] === "{dir}/a.txt" ? "literal" : "bad"; echo ":";
$all = glob("{dir}/*");
echo in_array("{dir}/.hidden.txt", $all) ? "bad" : "hidden"; echo ":";
$call = call_user_func("glob", "{dir}/*.log");
echo count($call) === 1 && basename($call[0]) === "b.log" ? "callglob" : "bad"; echo ":";
$call_array = call_user_func_array("glob", ["pattern" => "{dir}/*.txt"]);
echo count($call_array) === 2 ? "callarray" : "bad"; echo ":";
unlink("{dir}/.hidden.txt");
unlink("{dir}/c.txt");
unlink("{dir}/b.log");
unlink("{dir}/a.txt");
echo rmdir("{dir}") ? "cleanup" : "bad"; echo ":";
echo function_exists("glob");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(format!("{dir}/.hidden.txt"));
    let _ = std::fs::remove_file(format!("{dir}/c.txt"));
    let _ = std::fs::remove_file(format!("{dir}/b.log"));
    let _ = std::fs::remove_file(format!("{dir}/a.txt"));
    let _ = std::fs::remove_dir(&dir);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(format!("{dir}/.hidden.txt"));
    let _ = std::fs::remove_file(format!("{dir}/c.txt"));
    let _ = std::fs::remove_file(format!("{dir}/b.log"));
    let _ = std::fs::remove_file(format!("{dir}/a.txt"));
    let _ = std::fs::remove_dir(&dir);
    assert_eq!(
        values.output,
        "glob:empty:literal:hidden:callglob:callarray:cleanup:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval file-modification builtins update modes, masks, temp files, and dispatch.
#[test]
fn execute_program_dispatches_file_modify_builtins() {
    let pid = std::process::id();
    let filename = format!("elephc_magician_modify_{pid}.txt");
    let missing = format!("elephc_magician_modify_missing_{pid}.txt");
    let prefix = format!("evm{pid}_");
    let call_prefix = format!("evc{pid}_");
    let source = format!(
        r#"file_put_contents("{filename}", "x");
echo chmod(filename: "{filename}", permissions: 384) ? "chmod" : "bad"; echo ":";
echo (fileperms("{filename}") & 511) === 384 ? "mode" : "bad"; echo ":";
echo chmod("{missing}", 384) ? "bad" : "chmod-false"; echo ":";
$tmp = tempnam(directory: ".", prefix: "{prefix}");
echo file_exists($tmp) && str_starts_with(basename($tmp), "{prefix}") ? "tempnam" : "bad"; echo ":";
unlink($tmp);
$previous = umask(mask: 18);
$set = umask($previous);
echo $set === 18 ? "umask" : "bad"; echo ":";
$before = umask(18);
$probe = umask();
$restore = umask($before);
echo $probe === 18 && $restore === 18 ? "probe" : "bad"; echo ":";
echo call_user_func("chmod", "{filename}", 420) ? "callchmod" : "bad"; echo ":";
$call_tmp = call_user_func_array("tempnam", ["directory" => ".", "prefix" => "{call_prefix}"]);
echo file_exists($call_tmp) && str_starts_with(basename($call_tmp), "{call_prefix}") ? "calltempnam" : "bad"; echo ":";
unlink($call_tmp);
echo unlink("{filename}") ? "cleanup" : "bad"; echo ":";
echo function_exists("chmod"); echo function_exists("tempnam"); echo function_exists("umask");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&missing);
    for entry in std::fs::read_dir(".").expect("read eval test cwd") {
        let entry = entry.expect("read eval temp entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) || name.starts_with(&call_prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&missing);
    for entry in std::fs::read_dir(".").expect("read eval test cwd") {
        let entry = entry.expect("read eval temp entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) || name.starts_with(&call_prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    assert_eq!(
        values.output,
        "chmod:mode:chmod-false:tempnam:umask:probe:callchmod:calltempnam:cleanup:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval ownership builtins mutate local files and dispatch dynamically.
#[test]
fn execute_program_dispatches_file_ownership_builtins() {
    let pid = std::process::id();
    let filename = format!("elephc_magician_ownership_{pid}.txt");
    let link = format!("elephc_magician_ownership_link_{pid}.txt");
    let missing = format!("elephc_magician_ownership_missing_{pid}.txt");
    let uid = unsafe { libc::geteuid() };
    let gid = unsafe { libc::getegid() };
    let source = format!(
        r#"file_put_contents("{filename}", "x");
echo symlink("{filename}", "{link}") ? "symlink" : "bad"; echo ":";
echo chown("{filename}", {uid}) ? "chown" : "bad"; echo ":";
echo chgrp(filename: "{filename}", group: {gid}) ? "chgrp" : "bad"; echo ":";
echo lchown("{link}", {uid}) ? "lchown" : "bad"; echo ":";
echo lchgrp(filename: "{link}", group: {gid}) ? "lchgrp" : "bad"; echo ":";
echo chown("{missing}", {uid}) ? "bad" : "missing"; echo ":";
echo chown("{filename}", "__elephc_eval_missing_user__") ? "bad" : "user-false"; echo ":";
echo chgrp("{filename}", "__elephc_eval_missing_group__") ? "bad" : "group-false"; echo ":";
echo call_user_func("chgrp", "{filename}", {gid}) ? "callchgrp" : "bad"; echo ":";
echo call_user_func_array("lchown", ["filename" => "{link}", "user" => {uid}]) ? "arraylchown" : "bad"; echo ":";
echo unlink("{link}") && unlink("{filename}") ? "cleanup" : "bad"; echo ":";
echo function_exists("chown"); echo function_exists("chgrp"); echo function_exists("lchown");
return function_exists("lchgrp");"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&missing);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&missing);
    assert_eq!(
        values.output,
        "symlink:chown:chgrp:lchown:lchgrp:missing:user-false:group-false:callchgrp:arraylchown:cleanup:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `touch()` creates files, stamps mtimes, and dispatches dynamically.
#[test]
fn execute_program_dispatches_touch_builtin() {
    let pid = std::process::id();
    let created = format!("elephc_magician_touch_created_{pid}.txt");
    let stamped = format!("elephc_magician_touch_stamped_{pid}.txt");
    let missing = format!("elephc_magician_touch_missing_{pid}/x.txt");
    let source = format!(
        r#"echo touch(filename: "{created}") && file_exists("{created}") ? "create" : "bad"; echo ":";
file_put_contents("{stamped}", "x");
echo touch("{stamped}", 1000000000) ? "mtime" : "bad"; echo ":";
echo filemtime("{stamped}") === 1000000000 ? "readmtime" : "bad"; echo ":";
echo touch("{stamped}", 1000000001, null) && filemtime("{stamped}") === 1000000001 ? "nullatime" : "bad"; echo ":";
echo touch("{stamped}", 1000000002, 1000000003) && filemtime("{stamped}") === 1000000002 ? "both" : "bad"; echo ":";
echo touch("{missing}") ? "bad" : "touch-false"; echo ":";
echo call_user_func("touch", "{created}", 1000000004) ? "calltouch" : "bad"; echo ":";
echo call_user_func_array("touch", ["filename" => "{stamped}", "mtime" => 1000000005]) ? "callarray" : "bad"; echo ":";
echo unlink("{created}") && unlink("{stamped}") ? "cleanup" : "bad"; echo ":";
echo function_exists("touch");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&created);
    let _ = std::fs::remove_file(&stamped);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&created);
    let _ = std::fs::remove_file(&stamped);
    assert_eq!(
        values.output,
        "create:mtime:readmtime:nullatime:both:touch-false:calltouch:callarray:cleanup:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
