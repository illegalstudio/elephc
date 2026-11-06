//! Purpose:
//! Regresses callback discovery through rooted stream-context literal hashes.
//!
//! Called from:
//! - The I/O codegen integration suite.
//!
//! Key details:
//! - Mixed literal entries retain raw callable descriptors and their producer identity.
//! - Conditional values and exception handlers cannot hide an initialized literal callback.

use crate::support::*;

/// Heterogeneous params keep the literal notification descriptor visible and alive.
#[test]
fn test_stream_callback_owners_mixed_literal() {
    assert_eq!(compile_and_run(r#"<?php
$ctx = stream_context_create([], [
    'notification' => function($code) { echo 'M', $code, ';'; },
    'options' => []
]);
$stream = fopen('http://127.0.0.1:1/', 'r');
echo $stream === false ? 'closed' : 'open';
"#), "M9;closed");
}

/// Raw callable tags and roots do not depend on EIR optimization.
#[test]
fn test_stream_callback_owners_mixed_literal_no_ir_opt() {
    without_ir_opt(test_stream_callback_owners_mixed_literal);
}

/// A conditional sibling entry inside try/catch preserves the literal hash's initialized root.
#[test]
fn test_stream_callback_owners_conditional_sibling() {
    assert_eq!(compile_and_run(r#"<?php
try {
    $ctx = stream_context_set_params(
        params: ['notification' => function($code) { echo 'B', $code, ';'; },
            'options' => $argc > 0 ? ['http' => ['timeout' => 1]] : []],
        context: stream_context_create()
    );
    $stream = fopen('http://127.0.0.1:1/', 'r');
    echo $stream === false ? 'closed' : 'open';
} catch (Throwable $error) { echo 'unexpected'; }
"#), "B9;closed");
}

/// Branch-aware callback provenance is independent of EIR optimization.
#[test]
fn test_stream_callback_owners_conditional_sibling_no_ir_opt() {
    without_ir_opt(test_stream_callback_owners_conditional_sibling);
}
