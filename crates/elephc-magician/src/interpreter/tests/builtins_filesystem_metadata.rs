//! Purpose:
//! Interpreter tests for filesystem path, metadata, stat, and space builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases use temporary files while keeping filesystem metadata assertions focused.

use super::super::*;
use super::support::*;

/// Verifies eval path component builtins mirror static basename/dirname edge cases.
#[test]
fn execute_program_dispatches_path_component_builtins() {
    let program = parse_fragment(
        br#"echo basename("/var/log/syslog.log", ".log") . ":";
echo basename(path: "/usr///") . ":";
echo basename("/", "x") === "" ? "root" : "bad"; echo ":";
echo dirname("/usr/local/bin/tool", 2) . ":";
echo dirname(path: "/usr///local///bin") . ":";
echo call_user_func("basename", "foo.tar.gz", ".bz2") . ":";
echo call_user_func_array("dirname", ["path" => "/usr", "levels" => 3]) . ":";
echo function_exists("basename");
return function_exists("dirname");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "syslog:usr:root:/usr/local:/usr///local:foo.tar.gz:/:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `realpath()` resolves existing paths and returns false for misses.
#[test]
fn execute_program_dispatches_realpath_builtin() {
    let program = parse_fragment(
            br#"echo realpath(".") !== false ? "resolved" : "bad"; echo ":";
echo realpath(path: "elephc-magician-missing-path") === false ? "false" : "bad"; echo ":";
echo call_user_func("realpath", ".") !== false ? "call" : "bad"; echo ":";
echo call_user_func_array("realpath", ["path" => "elephc-magician-missing-path"]) === false ? "array-false" : "bad";
echo ":";
return function_exists("realpath");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "resolved:false:call:array-false:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `fnmatch()` supports wildcards, classes, flags, constants, and callables.
#[test]
fn execute_program_dispatches_fnmatch_builtin() {
    let program = parse_fragment(
            br#"echo fnmatch("*.log", "system.log") ? "match" : "bad"; echo ":";
echo fnmatch("*.log", "logs/system.log", FNM_PATHNAME) ? "bad" : "path"; echo ":";
echo fnmatch("*.LOG", "system.log", FNM_CASEFOLD) ? "case" : "bad"; echo ":";
echo fnmatch("*", ".env", FNM_PERIOD) ? "bad" : "period"; echo ":";
echo fnmatch("[!abc]oo", "doo") && !fnmatch("[!abc]oo", "boo") ? "class" : "bad"; echo ":";
echo fnmatch('a\\*b', 'a*b') ? "escape" : "bad"; echo ":";
echo fnmatch('a\\*b', 'a\\xxb', FNM_NOESCAPE) ? "noescape" : "bad"; echo ":";
$flags = FNM_PATHNAME | FNM_CASEFOLD;
echo fnmatch("dir/*.TXT", "dir/file.txt", $flags) ? "flags" : "bad"; echo ":";
echo call_user_func("fnmatch", "*.txt", "report.txt") ? "call" : "bad"; echo ":";
echo call_user_func_array("fnmatch", ["pattern" => "*.TXT", "filename" => "report.txt", "flags" => FNM_CASEFOLD]) ? "callarray" : "bad"; echo ":";
echo function_exists("fnmatch"); echo defined("FNM_CASEFOLD");
return FNM_CASEFOLD;"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "match:path:case:period:class:escape:noescape:flags:call:callarray:11"
    );
    assert_eq!(values.get(result), FakeValue::Int(EVAL_FNM_CASEFOLD));
}
/// Verifies eval `pathinfo()` handles arrays, component flags, constants, and callables.
#[test]
fn execute_program_dispatches_pathinfo_builtin() {
    let program = parse_fragment(
            br#"$info = pathinfo("/var/log/syslog.log");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . ":";
echo pathinfo("archive.tar.gz", PATHINFO_EXTENSION) . ":";
echo pathinfo(".bashrc", PATHINFO_FILENAME) === "" ? "dotfile" : "bad"; echo ":";
echo pathinfo("file.", PATHINFO_EXTENSION) === "" ? "trail" : "bad"; echo ":";
echo pathinfo("", PATHINFO_DIRNAME) === "" ? "empty-dir" : "bad"; echo ":";
$plain = pathinfo("/etc/hosts");
echo array_key_exists("extension", $plain) ? "bad" : "no-ext"; echo ":";
echo pathinfo("/a/b.php", PATHINFO_BASENAME | PATHINFO_FILENAME) . ":";
$call = call_user_func("pathinfo", "foo.txt", PATHINFO_ALL);
echo $call["basename"] . ":";
echo call_user_func_array("pathinfo", ["path" => "foo.txt", "flags" => 0]) === "" ? "zero" : "bad";
echo ":"; echo function_exists("pathinfo"); echo defined("PATHINFO_ALL");
return PATHINFO_ALL;"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "/var/log|syslog.log|log|syslog:gz:dotfile:trail:empty-dir:no-ext:b.php:foo.txt:zero:11"
    );
    assert_eq!(values.get(result), FakeValue::Int(EVAL_PATHINFO_ALL));
}
/// Verifies eval local filesystem builtins read, write, stat, delete, and dispatch.
#[test]
fn execute_program_dispatches_filesystem_builtins() {
    let filename = format!("elephc_magician_fs_probe_{}.txt", std::process::id());
    let missing = format!("elephc_magician_fs_missing_{}.txt", std::process::id());
    let source = format!(
        r#"echo file_put_contents("{filename}", "hello") . ":";
echo file_get_contents("{filename}") . ":";
echo file_exists("{filename}") ? "exists" : "missing"; echo ":";
echo is_file(filename: "{filename}") ? "file" : "bad"; echo ":";
echo is_dir(".") ? "dir" : "bad"; echo ":";
echo is_readable("{filename}") ? "readable" : "bad"; echo ":";
echo is_writable("{filename}") ? "writable" : "bad"; echo ":";
echo is_writeable("{filename}") ? "writeable" : "bad"; echo ":";
echo filesize("{filename}") . ":";
echo file_get_contents("{missing}") === false ? "missing-false" : "bad"; echo ":";
echo call_user_func("file_exists", "{filename}") ? "call-exists" : "bad"; echo ":";
echo call_user_func_array("filesize", ["filename" => "{filename}"]) . ":";
echo unlink("{filename}") ? "unlinked" : "bad"; echo ":";
echo file_exists("{filename}") ? "bad" : "gone"; echo ":";
echo function_exists("file_get_contents"); echo function_exists("file_put_contents");
echo function_exists("file_exists"); echo function_exists("is_file"); echo function_exists("is_dir");
echo function_exists("is_readable"); echo function_exists("is_writable"); echo function_exists("is_writeable");
echo function_exists("filesize");
return function_exists("unlink");"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&filename);
    assert_eq!(
            values.output,
            "5:hello:exists:file:dir:readable:writable:writeable:5:missing-false:call-exists:5:unlinked:gone:111111111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval disk-space builtins query local filesystem capacity and dispatch dynamically.
#[test]
fn execute_program_dispatches_disk_space_builtins() {
    let program = parse_fragment(
            br#"echo disk_free_space(".") > 0 ? "free" : "bad"; echo ":";
echo disk_total_space(directory: ".") > 0 ? "total" : "bad"; echo ":";
echo disk_total_space(".") >= disk_free_space(".") ? "ordered" : "bad"; echo ":";
echo disk_free_space("no/such/path/elephc-magician") === 0.0 ? "missing" : "bad"; echo ":";
echo call_user_func("disk_free_space", ".") > 0 ? "call" : "bad"; echo ":";
echo call_user_func_array("disk_total_space", ["directory" => "."]) > 0 ? "spread" : "bad"; echo ":";
echo function_exists("disk_free_space");
return function_exists("disk_total_space");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "free:total:ordered:missing:call:spread:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval stat metadata builtins expose scalar file metadata and link probes.
#[test]
fn execute_program_dispatches_stat_metadata_builtins() {
    let filename = format!("elephc_magician_stat_probe_{}.txt", std::process::id());
    let missing = format!("elephc_magician_stat_missing_{}.txt", std::process::id());
    let link = format!("elephc_magician_stat_link_{}.txt", std::process::id());
    let source = format!(
        r#"echo filemtime("{filename}") > 0 ? "mtime" : "bad"; echo ":";
echo fileatime("{filename}") > 0 ? "atime" : "bad"; echo ":";
echo filectime("{filename}") > 0 ? "ctime" : "bad"; echo ":";
echo fileperms("{filename}") > 0 ? "perms" : "bad"; echo ":";
echo fileowner("{filename}") >= 0 ? "owner" : "bad"; echo ":";
echo filegroup("{filename}") >= 0 ? "group" : "bad"; echo ":";
echo fileinode("{filename}") > 0 ? "inode" : "bad"; echo ":";
echo filetype("{filename}") . ":";
echo filetype(".") . ":";
echo filetype("{link}") . ":";
echo is_executable("{filename}") ? "bad" : "noexec"; echo ":";
echo is_link("{link}") ? "link" : "bad"; echo ":";
echo fileatime("{missing}") === false ? "missing-atime" : "bad"; echo ":";
echo filectime("{missing}") === false ? "missing-ctime" : "bad"; echo ":";
echo fileperms("{missing}") === false ? "missing-perms" : "bad"; echo ":";
echo fileowner("{missing}") === false ? "missing-owner" : "bad"; echo ":";
echo filegroup("{missing}") === false ? "missing-group" : "bad"; echo ":";
echo fileinode("{missing}") === false ? "missing-inode" : "bad"; echo ":";
echo filetype("{missing}") === false ? "missing-type" : "bad"; echo ":";
echo filemtime("{missing}") === 0 ? "missing-mtime" : "bad"; echo ":";
echo call_user_func("filetype", "{filename}") . ":";
echo call_user_func_array("fileinode", ["filename" => "{filename}"]) > 0 ? "callinode" : "bad"; echo ":";
echo function_exists("filemtime"); echo function_exists("fileatime");
echo function_exists("filectime"); echo function_exists("fileperms");
echo function_exists("fileowner"); echo function_exists("filegroup");
echo function_exists("fileinode"); echo function_exists("filetype");
echo function_exists("is_executable"); echo function_exists("is_link");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&link);
    std::fs::write(&filename, b"hello").expect("write stat fixture");
    std::os::unix::fs::symlink(&filename, &link).expect("create stat symlink");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&link);
    assert_eq!(
            values.output,
            "mtime:atime:ctime:perms:owner:group:inode:file:dir:link:noexec:link:missing-atime:missing-ctime:missing-perms:missing-owner:missing-group:missing-inode:missing-type:missing-mtime:file:callinode:1111111111"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `stat()` and `lstat()` build PHP-compatible metadata arrays.
#[test]
fn execute_program_dispatches_stat_array_builtins() {
    let pid = std::process::id();
    let filename = format!("elephc_magician_stat_array_{pid}.txt");
    let link = format!("elephc_magician_lstat_array_{pid}.txt");
    let missing = format!("elephc_magician_stat_array_missing_{pid}.txt");
    let source = format!(
        r#"$stat = stat("{filename}");
$lstat = lstat("{link}");
echo $stat["size"] === 5 && $stat[7] === $stat["size"] ? "stat" : "bad"; echo ":";
echo ($stat["mode"] & 61440) === 32768 ? "mode" : "bad"; echo ":";
echo ($lstat["mode"] & 61440) === 40960 ? "lstat" : "bad"; echo ":";
echo stat("{missing}") === false && lstat("{missing}") === false ? "missing" : "bad"; echo ":";
$call = call_user_func("stat", "{filename}");
echo $call["mtime"] === filemtime("{filename}") ? "callstat" : "bad"; echo ":";
$call_lstat = call_user_func_array("lstat", ["filename" => "{link}"]);
echo $call_lstat["ino"] > 0 ? "calllstat" : "bad"; echo ":";
echo unlink("{link}") && unlink("{filename}") ? "cleanup" : "bad"; echo ":";
echo function_exists("stat"); echo function_exists("lstat");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&link);
    std::fs::write(&filename, b"hello").expect("write stat array fixture");
    std::os::unix::fs::symlink(&filename, &link).expect("create stat array symlink");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&filename);
    let _ = std::fs::remove_file(&link);
    assert_eq!(
        values.output,
        "stat:mode:lstat:missing:callstat:calllstat:cleanup:11"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
