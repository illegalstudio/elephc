use super::*;

#[test]
fn test_require_once_chain_preserves_included_file_error_location() {
    let err = resolve_files_error(
        &[
            ("main.php", "<?php\nrequire_once 'a.php';\n"),
            ("a.php", "<?php\nrequire_once 'nested/b.php';\n"),
            ("nested/b.php", "<?php\nfunction broken() {\n    echo 1\n}\n"),
        ],
        "main.php",
    );

    assert_eq!(err.span.line, 4, "expected parser error to point into nested/b.php");
    assert_ne!(err.span.line, 2, "error should not point back to the require_once line");
    assert!(
        Path::new(err.file.as_deref().expect("expected included file path")).ends_with("nested/b.php"),
        "expected file path to reference nested/b.php, got {:?}",
        err.file,
    );
    assert!(
        err.message.contains("Expected ';'"),
        "unexpected error message: {}",
        err.message,
    );
    assert!(
        err.to_string().contains("nested/b.php:4"),
        "expected display output to include nested/b.php:4, got {}",
        err,
    );
}

// --- Float/math function errors ---

#[test]
fn test_error_include_missing_path() {
    // Empty `include ;` — parse_expr immediately sees `;` and errors out.
    expect_error("<?php include ;", "Unexpected token");
}

#[test]
fn test_error_include_non_string_path() {
    // Non-foldable path — parses fine but the resolver rejects it because
    // an integer literal is not a compile-time-constant *string*.
    let err = resolver_error("<?php include 42;");
    assert!(
        err.message.contains("compile-time-constant string"),
        "message did not mention compile-time-constant string: {}",
        err.message
    );
}

// --- INF/NAN function errors ---

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
    expect_error(
        r#"<?php fopen("file");"#,
        "fopen() takes exactly 2 arguments",
    );
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
    expect_error(
        "<?php file_get_contents();",
        "file_get_contents() takes exactly 1 argument",
    );
}

#[test]
fn test_error_file_get_contents_false_return_rejects_string_return_type() {
    expect_error(
        r#"<?php
function read_file(): string {
    return file_get_contents("missing.txt");
}
"#,
        "Function 'read_file' return type expects Str, got Union([Str, Bool])",
    );
}

#[test]
fn test_error_file_put_contents_wrong_args() {
    expect_error(
        r#"<?php file_put_contents("x");"#,
        "file_put_contents() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_file_exists_wrong_args() {
    expect_error(
        "<?php file_exists();",
        "file_exists() takes exactly 1 argument",
    );
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
    expect_error(
        r#"<?php rename("x");"#,
        "rename() takes exactly 2 arguments",
    );
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
    expect_error(
        r#"<?php tempnam("x");"#,
        "tempnam() takes exactly 2 arguments",
    );
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
    expect_error(
        "<?php is_readable();",
        "is_readable() takes exactly 1 argument",
    );
}

#[test]
fn test_error_is_writable_wrong_args() {
    expect_error(
        "<?php is_writable();",
        "is_writable() takes exactly 1 argument",
    );
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
    expect_error(
        "<?php sys_get_temp_dir(1);",
        "sys_get_temp_dir() takes no arguments",
    );
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
    expect_error(
        r#"<?php readline(1, 2);"#,
        "readline() takes 0 or 1 arguments",
    );
}

#[test]
fn test_error_fgetcsv_wrong_args() {
    expect_error("<?php fgetcsv();", "fgetcsv() takes 1 to 3 arguments");
}

#[test]
fn test_error_fputcsv_wrong_args() {
    expect_error("<?php fputcsv(1);", "fputcsv() takes 2 to 4 arguments");
}

#[test]
fn test_error_dirname_wrong_args() {
    expect_error("<?php dirname();", "dirname() takes 1 or 2 arguments");
}

#[test]
fn test_error_dirname_rejects_static_levels_below_one() {
    expect_error(
        r#"<?php dirname("/tmp/file", 0);"#,
        "dirname() levels must be greater than or equal to 1",
    );
}

#[test]
fn test_error_fnmatch_wrong_args() {
    expect_error("<?php fnmatch(\"*.txt\");", "fnmatch() takes 2 or 3 arguments");
}

#[test]
fn test_error_fnmatch_rejects_unsupported_flags() {
    expect_error(
        r#"<?php fnmatch("*.TXT", "file.txt", 16);"#,
        "fnmatch() flags other than 0 are not supported yet",
    );
}

#[test]
fn test_error_pathinfo_rejects_dynamic_flags() {
    expect_error(
        r#"<?php
$flag = PATHINFO_EXTENSION;
echo pathinfo("foo.txt", $flag);
"#,
        "pathinfo() flag must be a compile-time PATHINFO_* constant, bitmask, or integer literal",
    );
}

// --- v0.6: switch/match/array errors ---

#[test]
fn test_error_global_missing_var() {
    expect_error("<?php global ;", "Expected variable after 'global'");
}
