//! Purpose:
//! Integration or regression tests for diagnostic coverage of I/O builtin streams, including var dump wrong args, print r wrong args, and fopen wrong args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies var_dump() produces correct error when called with no arguments.
#[test]
fn test_error_var_dump_wrong_args() {
    expect_error("<?php var_dump();", "var_dump() takes exactly 1 argument");
}

/// Verifies print_r() produces correct error when called with no arguments.
#[test]
fn test_error_print_r_wrong_args() {
    expect_error("<?php print_r();", "print_r() takes exactly 1 argument");
}

/// Verifies fopen() produces correct error when called with only one argument.
#[test]
fn test_error_fopen_wrong_args() {
    expect_error(
        r#"<?php fopen("file");"#,
        "fopen() takes 2 to 4 arguments",
    );
}

/// Verifies fclose() produces correct error when called with no arguments.
#[test]
fn test_error_fclose_wrong_args() {
    expect_error("<?php fclose();", "fclose() takes exactly 1 argument");
}

/// Verifies fclose() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fclose_requires_resource_handle() {
    expect_error("<?php fclose(1);", "fclose() expects resource, got int");
}

/// Verifies fread() produces correct error when called with only one argument.
#[test]
fn test_error_fread_wrong_args() {
    expect_error("<?php fread(1);", "fread() takes exactly 2 arguments");
}

/// Verifies fread() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fread_requires_resource_handle() {
    expect_error("<?php fread(1, 1);", "fread() expects resource, got int");
}

/// Verifies fwrite() produces correct error when called with only one argument.
#[test]
fn test_error_fwrite_wrong_args() {
    expect_error("<?php fwrite(1);", "fwrite() takes exactly 2 arguments");
}

/// Verifies fwrite() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fwrite_requires_resource_handle() {
    expect_error(
        r#"<?php fwrite(1, "x");"#,
        "fwrite() expects resource, got int",
    );
}

/// Verifies fgets() produces correct error when called with no arguments.
#[test]
fn test_error_fgets_wrong_args() {
    expect_error("<?php fgets();", "fgets() takes exactly 1 argument");
}

/// Verifies fgets() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fgets_requires_resource_handle() {
    expect_error("<?php fgets(1);", "fgets() expects resource, got int");
}

/// Verifies fgetc() produces correct error when called with no arguments.
#[test]
fn test_error_fgetc_wrong_args() {
    expect_error("<?php fgetc();", "fgetc() takes exactly 1 argument");
}

/// Verifies fgetc() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fgetc_requires_resource_handle() {
    expect_error("<?php fgetc(1);", "fgetc() expects resource, got int");
}

/// Verifies fpassthru() produces correct error when called with no arguments.
#[test]
fn test_error_fpassthru_wrong_args() {
    expect_error("<?php fpassthru();", "fpassthru() takes exactly 1 argument");
}

/// Verifies fpassthru() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fpassthru_requires_resource_handle() {
    expect_error("<?php fpassthru(1);", "fpassthru() expects resource, got int");
}

/// Verifies flock() produces correct error when called with only STDIN (1 argument, requires 2 or 3).
#[test]
fn test_error_flock_wrong_args() {
    expect_error("<?php flock(STDIN);", "flock() takes 2 or 3 arguments");
}

/// Verifies flock() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_flock_requires_resource_handle() {
    expect_error("<?php flock(1, LOCK_EX);", "flock() expects resource, got int");
}

/// Verifies flock() produces correct error when the operation argument is a string instead of int.
#[test]
fn test_error_flock_rejects_non_int_operation() {
    expect_error(
        r#"<?php flock(STDIN, "exclusive");"#,
        "flock() operation must be int",
    );
}

/// Verifies flock() produces correct error when $would_block is not passed a variable.
#[test]
fn test_error_flock_would_block_requires_variable() {
    expect_error(
        r#"<?php flock(STDIN, LOCK_EX, 0);"#,
        "flock() parameter $would_block must be passed a variable",
    );
}

/// Verifies tmpfile() produces correct error when called with an argument.
#[test]
fn test_error_tmpfile_wrong_args() {
    expect_error("<?php tmpfile(1);", "tmpfile() takes no arguments");
}

/// Verifies tmpfile() produces correct error when called with a non-empty spread argument.
#[test]
fn test_error_tmpfile_rejects_nonempty_static_spread() {
    expect_error("<?php tmpfile(...[1]);", "tmpfile() takes no arguments");
}

/// Verifies a function with string return type annotation produces an error when returning fgetc() which can return false.
#[test]
fn test_error_fgetc_false_return_rejects_string_return_type() {
    expect_error(
        r#"<?php
function read_char(): string {
    return fgetc(STDIN);
}
"#,
        "Function 'read_char' return type expects Str, got Union([Str, Bool])",
    );
}

/// Verifies feof() produces correct error when called with no arguments.
#[test]
fn test_error_feof_wrong_args() {
    expect_error("<?php feof();", "feof() takes exactly 1 argument");
}

/// Verifies fstat() produces correct error when passed an int instead of a resource.
#[test]
fn test_error_fstat_requires_resource_handle() {
    expect_error("<?php fstat(-1);", "fstat() expects resource, got int");
}

/// Verifies ftruncate(), fsync(), fflush(), and fdatasync() produce correct errors when called with wrong argument count.
#[test]
fn test_error_stream_modify_builtins_wrong_args() {
    for (source, message) in [
        ("<?php ftruncate(1);", "ftruncate() takes exactly 2 arguments"),
        ("<?php fsync();", "fsync() takes exactly 1 argument"),
        ("<?php fflush();", "fflush() takes exactly 1 argument"),
        ("<?php fdatasync();", "fdatasync() takes exactly 1 argument"),
    ] {
        expect_error(source, message);
    }
}

/// Verifies ftruncate(), fsync(), fflush(), and fdatasync() produce correct errors when passed an int instead of a resource.
#[test]
fn test_error_stream_modify_builtins_require_resource_handle() {
    for (source, message) in [
        ("<?php ftruncate(1, 0);", "ftruncate() expects resource, got int"),
        ("<?php fsync(1);", "fsync() expects resource, got int"),
        ("<?php fflush(1);", "fflush() expects resource, got int"),
        ("<?php fdatasync(1);", "fdatasync() expects resource, got int"),
    ] {
        expect_error(source, message);
    }
}

/// Verifies the invalid-call diagnostic for error is resource wrong args.
#[test]
fn test_error_is_resource_wrong_args() {
    expect_error(
        "<?php is_resource();",
        "is_resource() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error get resource type wrong args.
#[test]
fn test_error_get_resource_type_wrong_args() {
    expect_error(
        "<?php get_resource_type();",
        "get_resource_type() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error get resource id wrong args.
#[test]
fn test_error_get_resource_id_wrong_args() {
    expect_error(
        "<?php get_resource_id(STDIN, STDOUT);",
        "get_resource_id() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error stream isatty wrong args.
#[test]
fn test_error_stream_isatty_wrong_args() {
    expect_error(
        "<?php stream_isatty();",
        "stream_isatty() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error stream isatty requires resource handle.
#[test]
fn test_error_stream_isatty_requires_resource_handle() {
    expect_error(
        "<?php stream_isatty(1);",
        "stream_isatty() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream supports lock requires resource handle.
#[test]
fn test_error_stream_supports_lock_requires_resource_handle() {
    expect_error(
        "<?php stream_supports_lock(1);",
        "stream_supports_lock() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream is local wrong args.
#[test]
fn test_error_stream_is_local_wrong_args() {
    expect_error(
        "<?php stream_is_local();",
        "stream_is_local() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error stream get contents wrong args:
/// both zero args and more than three args are rejected (the optional `$length`
/// and `$offset` widened the arity to 1–3).
#[test]
fn test_error_stream_get_contents_wrong_args() {
    expect_error(
        "<?php stream_get_contents();",
        "stream_get_contents() takes 1 to 3 arguments",
    );
    expect_error(
        "<?php stream_get_contents(STDIN, 1, 2, 3);",
        "stream_get_contents() takes 1 to 3 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream get contents requires resource handle.
#[test]
fn test_error_stream_get_contents_requires_resource_handle() {
    expect_error(
        "<?php stream_get_contents(1);",
        "stream_get_contents() expects resource, got int",
    );
}

/// Verifies `stream_get_contents()` rejects non-integer length and offset
/// arguments before codegen lowers them as raw integer registers.
#[test]
fn test_error_stream_get_contents_length_and_offset_must_be_ints() {
    expect_error(
        r#"<?php stream_get_contents(STDIN, "5");"#,
        "stream_get_contents() length must be int or null",
    );
    expect_error(
        r#"<?php stream_get_contents(STDIN, 5, "0");"#,
        "stream_get_contents() offset must be int",
    );
}

/// Verifies the invalid-call diagnostic for error stream copy to stream wrong args.
#[test]
fn test_error_stream_copy_to_stream_wrong_args() {
    expect_error(
        "<?php stream_copy_to_stream(STDIN);",
        "stream_copy_to_stream() takes 2 to 4 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream copy to stream requires resource handles.
#[test]
fn test_error_stream_copy_to_stream_requires_resource_handles() {
    expect_error(
        "<?php stream_copy_to_stream(STDIN, 1);",
        "stream_copy_to_stream() expects resource, got int",
    );
}

/// Verifies `stream_copy_to_stream()` rejects non-integer length and offset
/// arguments before the bounded-copy lowering consumes them.
#[test]
fn test_error_stream_copy_to_stream_length_and_offset_must_be_ints() {
    expect_error(
        r#"<?php stream_copy_to_stream(STDIN, STDOUT, "5");"#,
        "stream_copy_to_stream() length must be int or null",
    );
    expect_error(
        r#"<?php stream_copy_to_stream(STDIN, STDOUT, 5, "0");"#,
        "stream_copy_to_stream() offset must be int",
    );
}

/// Verifies the invalid-call diagnostic for error stream introspection lists take no args.
#[test]
fn test_error_stream_introspection_lists_take_no_args() {
    for (source, message) in [
        (
            "<?php stream_get_wrappers(1);",
            "stream_get_wrappers() takes no arguments",
        ),
        (
            "<?php stream_get_transports(1);",
            "stream_get_transports() takes no arguments",
        ),
        (
            "<?php stream_get_filters(1);",
            "stream_get_filters() takes no arguments",
        ),
    ] {
        expect_error(source, message);
    }
}

/// Verifies the invalid-call diagnostic for error stream socket server wrong args.
#[test]
fn test_error_stream_socket_server_wrong_args() {
    expect_error(
        "<?php stream_socket_server();",
        "stream_socket_server() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket client wrong args.
#[test]
fn test_error_stream_socket_client_wrong_args() {
    expect_error(
        "<?php stream_socket_client();",
        "stream_socket_client() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error fsockopen wrong args.
#[test]
fn test_error_fsockopen_wrong_args() {
    expect_error(
        "<?php fsockopen();",
        "fsockopen() takes 2 to 5 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error fsockopen error code not variable.
#[test]
fn test_error_fsockopen_error_code_not_variable() {
    expect_error(
        r#"<?php fsockopen("127.0.0.1", 80, 0);"#,
        "fsockopen() parameter $error_code must be passed a variable",
    );
}

/// Verifies the invalid-call diagnostic for error stream wrapper register wrong args.
#[test]
fn test_error_stream_wrapper_register_wrong_args() {
    expect_error(
        "<?php stream_wrapper_register();",
        "stream_wrapper_register() takes 2 or 3 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream wrapper register unknown class.
#[test]
fn test_error_stream_wrapper_register_unknown_class() {
    expect_error(
        r#"<?php stream_wrapper_register("missing", "MissingWrapper");"#,
        "stream_wrapper_register(): undefined class 'MissingWrapper'",
    );
}

/// Verifies the invalid-call diagnostic for error stream wrapper unregister wrong args.
#[test]
fn test_error_stream_wrapper_unregister_wrong_args() {
    expect_error(
        "<?php stream_wrapper_unregister();",
        "stream_wrapper_unregister() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error stream wrapper restore wrong args.
#[test]
fn test_error_stream_wrapper_restore_wrong_args() {
    expect_error(
        "<?php stream_wrapper_restore();",
        "stream_wrapper_restore() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket enable crypto wrong args.
#[test]
fn test_error_stream_socket_enable_crypto_wrong_args() {
    expect_error(
        "<?php stream_socket_enable_crypto();",
        "stream_socket_enable_crypto() takes 2 to 4 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream filter register wrong args.
#[test]
fn test_error_stream_filter_register_wrong_args() {
    expect_error(
        "<?php stream_filter_register();",
        "stream_filter_register() takes exactly 2 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream filter register unknown class.
#[test]
fn test_error_stream_filter_register_unknown_class() {
    expect_error(
        r#"<?php stream_filter_register("missing.filter", "MissingFilter");"#,
        "stream_filter_register(): undefined class 'MissingFilter'",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket accept wrong args.
#[test]
fn test_error_stream_socket_accept_wrong_args() {
    expect_error(
        "<?php stream_socket_accept();",
        "stream_socket_accept() takes 1 to 3 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket accept requires resource.
#[test]
fn test_error_stream_socket_accept_requires_resource() {
    expect_error(
        "<?php stream_socket_accept(1);",
        "stream_socket_accept() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream get line wrong args.
#[test]
fn test_error_stream_get_line_wrong_args() {
    expect_error(
        "<?php stream_get_line(STDIN);",
        "stream_get_line() takes 2 or 3 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream get line requires resource.
#[test]
fn test_error_stream_get_line_requires_resource() {
    expect_error(
        "<?php stream_get_line(1, 80);",
        "stream_get_line() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream set blocking wrong args.
#[test]
fn test_error_stream_set_blocking_wrong_args() {
    expect_error(
        "<?php stream_set_blocking(STDIN);",
        "stream_set_blocking() takes exactly 2 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream set blocking requires resource.
#[test]
fn test_error_stream_set_blocking_requires_resource() {
    expect_error(
        "<?php stream_set_blocking(1, true);",
        "stream_set_blocking() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket shutdown wrong args.
#[test]
fn test_error_stream_socket_shutdown_wrong_args() {
    expect_error(
        "<?php stream_socket_shutdown(STDIN);",
        "stream_socket_shutdown() takes exactly 2 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error gethostname wrong args.
#[test]
fn test_error_gethostname_wrong_args() {
    expect_error(
        "<?php gethostname(1);",
        "gethostname() takes no arguments",
    );
}

/// Verifies the invalid-call diagnostic for error gethostbyname wrong args.
#[test]
fn test_error_gethostbyname_wrong_args() {
    expect_error(
        "<?php gethostbyname();",
        "gethostbyname() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error gethostbyaddr wrong args.
#[test]
fn test_error_gethostbyaddr_wrong_args() {
    expect_error(
        "<?php gethostbyaddr();",
        "gethostbyaddr() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error stream filter append wrong args.
#[test]
fn test_error_stream_filter_append_wrong_args() {
    // Too few (1) and too many (5) arguments both fail; the optional 4th
    // `$params` argument is accepted (2..=4 args are valid).
    expect_error(
        "<?php stream_filter_append(STDIN);",
        "stream_filter_append() takes 2 to 4 arguments",
    );
    expect_error(
        "<?php stream_filter_append(STDIN, \"string.rot13\", STREAM_FILTER_ALL, 6, 7);",
        "stream_filter_append() takes 2 to 4 arguments",
    );
}

// stream_filter_append() with an unknown filter name no longer fails at
// compile time: unknown built-in names are routed through the user-filter
// registry (Phase 10 tier 3), and an unregistered name resolves to PHP
// false at runtime. The "unknown stream filter" compile-time error is
// retired; runtime behavior is verified in the codegen test
// `test_user_stream_filter_unknown_name_returns_false`.

/// Verifies the invalid-call diagnostic for error stream filter remove wrong args.
#[test]
fn test_error_stream_filter_remove_wrong_args() {
    expect_error(
        "<?php stream_filter_remove();",
        "stream_filter_remove() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error getprotobyname wrong args.
#[test]
fn test_error_getprotobyname_wrong_args() {
    expect_error(
        "<?php getprotobyname();",
        "getprotobyname() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error getprotobynumber wrong args.
#[test]
fn test_error_getprotobynumber_wrong_args() {
    expect_error(
        "<?php getprotobynumber();",
        "getprotobynumber() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error getservbyname wrong args.
#[test]
fn test_error_getservbyname_wrong_args() {
    expect_error(
        r#"<?php getservbyname("http");"#,
        "getservbyname() takes exactly 2 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error getservbyport wrong args.
#[test]
fn test_error_getservbyport_wrong_args() {
    expect_error(
        "<?php getservbyport(80);",
        "getservbyport() takes exactly 2 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream set timeout wrong args.
#[test]
fn test_error_stream_set_timeout_wrong_args() {
    expect_error(
        "<?php stream_set_timeout(STDIN);",
        "stream_set_timeout() takes 2 or 3 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream set timeout requires resource.
#[test]
fn test_error_stream_set_timeout_requires_resource() {
    expect_error(
        "<?php stream_set_timeout(1, 5);",
        "stream_set_timeout() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket sendto wrong args.
#[test]
fn test_error_stream_socket_sendto_wrong_args() {
    expect_error(
        "<?php stream_socket_sendto(STDIN);",
        "stream_socket_sendto() takes 2 to 4 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket sendto requires resource.
#[test]
fn test_error_stream_socket_sendto_requires_resource() {
    expect_error(
        r#"<?php stream_socket_sendto(1, "x");"#,
        "stream_socket_sendto() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket recvfrom wrong args.
#[test]
fn test_error_stream_socket_recvfrom_wrong_args() {
    expect_error(
        "<?php stream_socket_recvfrom(STDIN);",
        "stream_socket_recvfrom() takes 2 to 4 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket recvfrom requires resource.
#[test]
fn test_error_stream_socket_recvfrom_requires_resource() {
    expect_error(
        "<?php stream_socket_recvfrom(1, 64);",
        "stream_socket_recvfrom() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket recvfrom address not variable.
#[test]
fn test_error_stream_socket_recvfrom_address_not_variable() {
    expect_error(
        "<?php stream_socket_recvfrom(STDIN, 32, 0, \"literal\");",
        "stream_socket_recvfrom() parameter $address must be passed a variable",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket recvfrom address not string.
#[test]
fn test_error_stream_socket_recvfrom_address_not_string() {
    expect_error(
        "<?php $n = 1; stream_socket_recvfrom(STDIN, 32, 0, $n);",
        "stream_socket_recvfrom() parameter $address must be a string",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket get name wrong args.
#[test]
fn test_error_stream_socket_get_name_wrong_args() {
    expect_error(
        "<?php stream_socket_get_name(STDIN);",
        "stream_socket_get_name() takes exactly 2 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket get name requires resource.
#[test]
fn test_error_stream_socket_get_name_requires_resource() {
    expect_error(
        "<?php stream_socket_get_name(1, true);",
        "stream_socket_get_name() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket pair wrong args.
#[test]
fn test_error_stream_socket_pair_wrong_args() {
    expect_error(
        "<?php stream_socket_pair(1, 1);",
        "stream_socket_pair() takes exactly 3 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error popen wrong args.
#[test]
fn test_error_popen_wrong_args() {
    expect_error(
        r#"<?php popen("ls");"#,
        "popen() takes exactly 2 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error pclose requires resource.
#[test]
fn test_error_pclose_requires_resource() {
    expect_error(
        "<?php pclose(1);",
        "pclose() expects resource, got int",
    );
}

/// Verifies the invalid-call diagnostic for error opendir wrong args.
#[test]
fn test_error_opendir_wrong_args() {
    expect_error("<?php opendir();", "opendir() takes exactly 1 argument");
}

/// Verifies the invalid-call diagnostic for error readdir wrong args.
#[test]
fn test_error_readdir_wrong_args() {
    expect_error("<?php readdir();", "readdir() takes exactly 1 argument");
}

/// Verifies the invalid-call diagnostic for error readdir requires resource.
#[test]
fn test_error_readdir_requires_resource() {
    expect_error("<?php readdir(1);", "readdir() expects resource, got int");
}

/// Verifies the invalid-call diagnostic for error closedir requires resource.
#[test]
fn test_error_closedir_requires_resource() {
    expect_error("<?php closedir(1);", "closedir() expects resource, got int");
}

/// Verifies the invalid-call diagnostic for error rewinddir wrong args.
#[test]
fn test_error_rewinddir_wrong_args() {
    expect_error("<?php rewinddir();", "rewinddir() takes exactly 1 argument");
}

/// Verifies the invalid-call diagnostic for error stream select wrong args.
#[test]
fn test_error_stream_select_wrong_args() {
    expect_error(
        "<?php $a = []; stream_select($a);",
        "stream_select() takes 4 or 5 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream get meta data wrong args.
#[test]
fn test_error_stream_get_meta_data_wrong_args() {
    expect_error(
        "<?php stream_get_meta_data();",
        "stream_get_meta_data() takes exactly 1 argument",
    );
}

/// Verifies the invalid-call diagnostic for error stream get meta data requires resource.
#[test]
fn test_error_stream_get_meta_data_requires_resource() {
    expect_error(
        "<?php stream_get_meta_data(1);",
        "stream_get_meta_data() expects resource, got int",
    );
}
