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
    expect_error("<?php var_dump();", "var_dump() takes at least 1 argument");
}

/// Verifies print_r() produces correct error when called with no arguments.
#[test]
fn test_error_print_r_wrong_args() {
    expect_error("<?php print_r();", "print_r() takes 1 or 2 arguments");
}

/// Verifies fopen() produces correct error when called with only one argument.
#[test]
fn test_error_fopen_wrong_args() {
    expect_error(
        r#"<?php fopen("file");"#,
        "fopen() takes 2 to 4 arguments",
    );
}

/// Verifies the `while` guard narrowing is dropped once the loop exits.
///
/// The loop leaves precisely when the guard is false, so after it the variable is back to
/// `array|false` and a parameter declared `array` must refuse it. Without the restore the
/// narrowed array type outlives the loop, this snippet COMPILES, and the emitted code reads
/// a boxed `false` as an array — a silent wrong answer.
///
/// The witness is a typed parameter rather than `count()`, which is what this test used
/// until `count()` learned to raise PHP's TypeError at run time: with the runtime guard in
/// place, `count()` accepts a union with one countable member exactly as PHP does, so it can
/// no longer tell the narrowed type from the union. Verified by sentinel mutation in both
/// directions — dropping the restore makes this snippet compile, and a behavioural probe of
/// the same loop does NOT distinguish the two (both raise the TypeError), which is why this
/// stays an error test.
#[test]
fn test_error_while_guard_narrowing_does_not_outlive_the_loop() {
    expect_error(
        r#"<?php
function takesArray(array $a): int { return count($a); }
$h = fopen("x.csv", "r");
while (($row = fgetcsv($h)) !== false) {
}
echo takesArray($row);
"#,
        "expects Array(Mixed), got Union([Array(Mixed), False])",
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
///
/// The arity is 2 OR 3: php's signature is `fwrite($stream, string $data, ?int $length = null)`
/// and the third argument caps the write. php reports the shortfall at run time as
/// `ArgumentCountError: fwrite() expects at least 2 arguments, 1 given`; elephc refuses it at
/// compile time, as it does for every builtin arity.
#[test]
fn test_error_fwrite_wrong_args() {
    expect_error("<?php fwrite(1);", "fwrite() takes 2 or 3 arguments");
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
    // `fgets()` carries PHP's optional `$length`, so the arity it reports is a range.
    expect_error("<?php fgets();", "fgets() takes 1 or 2 arguments");
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
        "flock(): Argument #3 ($would_block) could not be passed by reference",
    );
}

/// Verifies formatted stream I/O builtins reject invalid argument counts.
#[test]
fn test_error_formatted_stream_io_wrong_args() {
    for (source, message) in [
        (
            "<?php fprintf(STDOUT);",
            "fprintf() takes at least 2 arguments",
        ),
        (
            r#"<?php vfprintf(STDOUT, "%s");"#,
            "vfprintf() takes exactly 3 arguments (stream, format, values)",
        ),
        (
            "<?php fscanf(STDIN);",
            "fscanf() takes at least 2 arguments",
        ),
    ] {
        expect_error(source, message);
    }
}

/// Regression: `fscanf()`'s by-ref `$vars` form is bounded, and says so past the bound.
///
/// The form itself works now — each arity is its own prelude function with that many
/// by-reference parameters, because a by-reference VARIADIC collects addresses into an array and
/// a write to an element replaces the address instead of following it. Nine variables reaches
/// past the last declared arity, and a call that cannot be lowered has to name the limit rather
/// than scan into nothing.
#[test]
fn test_error_fscanf_refuses_more_output_variables_than_it_declares() {
    expect_error(
        r#"<?php fscanf(STDIN, "%d %d %d %d %d %d %d %d %d", $a, $b, $c, $d, $e, $f, $g, $h, $i);"#,
        "fscanf(): at most 8 output variables are supported, got 9",
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
        "Function 'read_char' return type expects Str, got Union([Str, False])",
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

/// Verifies stream context and bucket helpers reject invalid argument counts.
#[test]
fn test_error_stream_context_and_bucket_wrong_args() {
    for (source, message) in [
        (
            "<?php stream_context_create([], [], []);",
            "stream_context_create() takes at most 2 arguments",
        ),
        (
            "<?php stream_context_get_default([], []);",
            "stream_context_get_default() takes at most 1 argument",
        ),
        (
            "<?php stream_context_set_default();",
            "stream_context_set_default() takes exactly 1 argument",
        ),
        (
            "<?php stream_context_set_option(STDIN);",
            "stream_context_set_option() takes 2 to 4 arguments",
        ),
        (
            "<?php stream_context_set_params(STDIN);",
            "stream_context_set_params() takes exactly 2 arguments",
        ),
        (
            "<?php stream_context_get_options();",
            "stream_context_get_options() takes exactly 1 argument",
        ),
        (
            "<?php stream_context_get_params();",
            "stream_context_get_params() takes exactly 1 argument",
        ),
        (
            "<?php stream_resolve_include_path();",
            "stream_resolve_include_path() takes exactly 1 argument",
        ),
        (
            "<?php stream_bucket_new(STDIN);",
            "stream_bucket_new() takes exactly 2 arguments",
        ),
        (
            "<?php stream_bucket_make_writeable();",
            "stream_bucket_make_writeable() takes exactly 1 argument",
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
        "stream_socket_server() takes 1 to 5 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error stream socket client wrong args.
#[test]
fn test_error_stream_socket_client_wrong_args() {
    expect_error(
        "<?php stream_socket_client();",
        "stream_socket_client() takes 1 to 6 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error fsockopen wrong args.
#[test]
fn test_error_fsockopen_wrong_args() {
    expect_error(
        "<?php fsockopen();",
        "fsockopen() takes 1 to 5 arguments",
    );
}

/// Verifies the invalid-call diagnostic for error fsockopen error code not variable.
#[test]
fn test_error_fsockopen_error_code_not_variable() {
    expect_error(
        r#"<?php fsockopen("127.0.0.1", 80, 0);"#,
        "fsockopen(): Argument #3 ($error_code) could not be passed by reference",
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

/// Verifies a `stream_wrapper_register()` naming an undeclared class COMPILES.
///
/// It used to be a compile error, which is the one thing php never makes it: MEASURED on
/// `php -n` 8.5.6 it throws a `TypeError` at run time, and a throw is catchable — so a program
/// that wraps the call in `try`/`catch` is valid php and has to build. The throw itself is
/// pinned by `test_a_wrapper_class_that_does_not_exist_throws_a_catchable_type_error`.
#[test]
fn test_error_stream_wrapper_register_unknown_class_is_not_a_compile_error() {
    expect_no_error(r#"<?php stream_wrapper_register("missing", "MissingWrapper");"#);
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

/// Verifies a `stream_filter_register()` naming an undeclared class COMPILES.
///
/// php REGISTERS it — MEASURED on `php -n` 8.5.6, the call answers `true` — and only the attach
/// fails, with two warnings. Refusing the program made a php script that runs unbuildable. The
/// run-time half is pinned by
/// `test_a_filter_class_that_does_not_exist_registers_and_fails_at_the_attach`.
#[test]
fn test_error_stream_filter_register_unknown_class_is_not_a_compile_error() {
    expect_no_error(r#"<?php stream_filter_register("missing.filter", "MissingFilter");"#);
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

/// Verifies the arity diagnostic for `stream_filter_prepend()` (2 to 4 args), mirroring append.
#[test]
fn test_error_stream_filter_prepend_wrong_args() {
    expect_error(
        "<?php stream_filter_prepend(STDIN);",
        "stream_filter_prepend() takes 2 to 4 arguments",
    );
}

/// Verifies the arity diagnostic for `stream_set_chunk_size()` (exactly 2 args).
#[test]
fn test_error_stream_set_chunk_size_wrong_args() {
    expect_error(
        "<?php stream_set_chunk_size(STDIN);",
        "stream_set_chunk_size() takes exactly 2 arguments",
    );
}

/// Verifies the arity diagnostic for `stream_set_read_buffer()` (exactly 2 args).
#[test]
fn test_error_stream_set_read_buffer_wrong_args() {
    expect_error(
        "<?php stream_set_read_buffer(STDIN);",
        "stream_set_read_buffer() takes exactly 2 arguments",
    );
}

/// Verifies the arity diagnostic for `stream_set_write_buffer()` (exactly 2 args).
#[test]
fn test_error_stream_set_write_buffer_wrong_args() {
    expect_error(
        "<?php stream_set_write_buffer(STDIN);",
        "stream_set_write_buffer() takes exactly 2 arguments",
    );
}

/// Verifies the arity diagnostic for `stream_bucket_prepend()` (exactly 2 args), mirroring append.
#[test]
fn test_error_stream_bucket_prepend_wrong_args() {
    expect_error(
        "<?php stream_bucket_prepend(1);",
        "stream_bucket_prepend() takes exactly 2 arguments",
    );
}

/// Verifies the arity diagnostic for `stream_bucket_append()` (exactly 2 args).
#[test]
fn test_error_stream_bucket_append_wrong_args() {
    expect_error(
        "<?php stream_bucket_append(1);",
        "stream_bucket_append() takes exactly 2 arguments",
    );
}

/// Verifies `pfsockopen()` requires a hostname and accepts at most five arguments.
///
/// Only the HOSTNAME is required: php's `$port` defaults to -1, which is what lets
/// `pfsockopen("unix:///path")` name a socket that has no port. This test previously asserted the
/// one-argument call was an ERROR, pinning a stricter rule than php's — a `unix://` connection
/// simply did not compile.
#[test]
fn test_error_pfsockopen_wrong_args() {
    expect_error("<?php pfsockopen();", "pfsockopen() takes 1 to 5 arguments");
    expect_error(
        "<?php pfsockopen(\"localhost\", 80, $e, $s, 1.0, 6);",
        "pfsockopen() takes 1 to 5 arguments",
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
        "stream_socket_recvfrom(): Argument #4 ($address) could not be passed by reference",
    );
}

/// Verifies a variable of the wrong type in the `$address` output position is rejected.
///
/// The diagnostic is now elephc's ordinary reassignment error rather than a message
/// `stream_socket_recvfrom()` spelled out for itself: `$address` is declared `ref(Str)`, so the
/// call binds the variable to `string` through the normal assignment merge. That is what makes
/// the SLOT a string wherever the runtime writes one, and it is the same diagnostic every other
/// by-reference output gives for the same mistake.
#[test]
fn test_error_stream_socket_recvfrom_address_accepts_a_retyped_variable() {
    // php lets a by-reference out-parameter change what its caller's variable holds: measured,
    // `$n = 1; stream_socket_recvfrom(STDIN, 32, 0, $n);` draws no complaint about `$n` at all.
    // This asserted a refusal instead, which is what elephc used to do.
    expect_no_error("<?php $n = 1; stream_socket_recvfrom(STDIN, 32, 0, $n);");
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
///
/// The wording follows the signature: php declares `opendir(string $directory, $context = null)`,
/// so since `$context` landed the range is 1 or 2, not exactly 1.
#[test]
fn test_error_opendir_wrong_args() {
    expect_error("<?php opendir();", "opendir() takes 1 or 2 arguments");
}

/// Verifies the invalid-call diagnostic for error readdir wrong args.
///
/// This test used to pin `readdir()` — the NO-argument form — as the refusal, which pinned a
/// bug: php declares `readdir(?resource $dir_handle = null)` and runs the handle-less call
/// against the last opened directory stream. The only arity php refuses is the second argument,
/// with `ArgumentCountError: readdir() expects at most 1 argument, 2 given` (MEASURED on
/// `php -n` 8.5.6); elephc says the same thing in its own house phrasing, at compile time.
#[test]
fn test_error_readdir_wrong_args() {
    expect_error(
        "<?php $h = opendir('.'); readdir($h, $h);",
        "readdir() takes at most 1 argument",
    );
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
///
/// `$dir_handle` is optional here too, so — as for `readdir()` — only a SECOND argument is out
/// of range: `ArgumentCountError: rewinddir() expects at most 1 argument, 2 given`.
#[test]
fn test_error_rewinddir_wrong_args() {
    expect_error(
        "<?php $h = opendir('.'); rewinddir($h, $h);",
        "rewinddir() takes at most 1 argument",
    );
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
