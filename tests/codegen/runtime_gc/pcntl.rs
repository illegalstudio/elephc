//! Purpose:
//! Ownership regressions for PCNTL output arrays and registered callable descriptors.
//!
//! Called from:
//! - `cargo test --test codegen_tests runtime_gc::pcntl` through Rust's test harness.
//!
//! Key details:
//! - Output rebinding, temporary signal normalization, and process-wide handler teardown
//!   must release every retained heap owner.

use crate::support::compile_and_run_with_heap_debug;

/// Output writers preserve native reference places after a body assignment boxes later reads.
#[test]
fn test_native_reference_output_snapshots_preserve_status_tag_and_aliases() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function nativeOutputSnapshots(int &$status, string &$tag, array &$matches): void {
    $status = 0;
    $pid = pcntl_waitpid(-1, $status, WNOHANG);
    $tag = 'before';
    $cipher = openssl_encrypt('payload', 'aes-128-gcm', '0123456789abcdef',
        OPENSSL_RAW_DATA, '123456789012', $tag);
    preg_match('/(a)/', 'a', $matches);
    echo $pid, ':', $status, ':', strlen($tag), ':', $matches[1], '|';
    echo openssl_decrypt($cipher, 'aes-128-gcm', '0123456789abcdef',
        OPENSSL_RAW_DATA, '123456789012', $tag), '|';
}
$status = 7; $tag = ''; $matches = [];
nativeOutputSnapshots($status, $tag, $matches);
echo $status, ':', strlen($tag), ':', $matches[1];
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "-1:0:16:a|payload|0:16:a", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}", out.stderr);
}

/// Reaps two children into the same usage local without leaking or double-freeing either hash.
#[test]
fn test_pcntl_waitpid_resource_usage_rebind_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        "<?php
        $usage = [];
        $first = pcntl_fork();
        if ($first === 0) { exit(3); }
        pcntl_waitpid($first, $status, 0, $usage);
        echo count($usage) . '|';
        $second = pcntl_fork();
        if ($second === 0) { exit(5); }
        pcntl_waitpid($second, $status, 0, $usage);
        echo pcntl_wexitstatus($status) . '|' . count($usage);",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "17|5|17", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr,
    );
}

/// Releases PHP 8.5 `waitid` usage written into a previously undefined local at frame exit.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_waitid_resource_usage_writeback_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(7); }
        $ok = pcntl_waitid(
            idtype: P_PID,
            id: $pid,
            flags: WEXITED,
            resource_usage: $usage,
        );
        echo ($ok ? 'ok' : 'bad') . '|' . count($usage);",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok|17", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr,
    );
}

/// Releases a capturing closure retained by the process-wide signal-handler table at exit.
#[test]
fn test_pcntl_signal_closure_registration_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        "<?php
        $prefix = 'signal';
        $handler = function (int $signal, array $info) use ($prefix): void {
            echo $prefix . ':' . $signal;
        };
        echo pcntl_signal(SIGUSR1, $handler) ? 'registered' : 'bad';",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "registered", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr,
    );
}

/// Releases every temporary integer array created for variable numeric-string signal sets.
#[test]
fn test_pcntl_signal_string_array_normalization_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        "<?php
        $signals = ['user' => '9', 'term' => '15'];
        for ($i = 0; $i < 3; $i++) {
            pcntl_sigprocmask(SIG_BLOCK, $signals);
            pcntl_sigprocmask(SIG_UNBLOCK, $signals);
        }
        echo 'clean';",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "clean", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr,
    );
}
