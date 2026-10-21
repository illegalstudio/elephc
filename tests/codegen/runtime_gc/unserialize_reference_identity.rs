//! Purpose:
//! Check writable identity and ownership of uppercase-R unserialized reference sets.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Reference promotion transfers the old payload owner into a nested Mixed child.
//! - Plain copies without serialized references must retain normal array COW behavior.

use crate::support::*;

/// Repeated decoding preserves R aliases without leaking promoted boxes or their payloads.
#[test]
fn test_unserialize_reference_identity_and_owned_payloads() {
    let source = r#"<?php
for ($i = 0; $i < 20; $i++) {
    $x = unserialize('a:2:{s:1:"a";a:1:{i:0;i:1;}s:1:"b";R:2;}');
    $x['a'][0] = 99; echo $x['b'][0], ':';
    $x['a'] = [8]; echo $x['b'][0], ':';
    $x['a'] = 7; echo $x['b'], '|'; unset($x);
    $plain = unserialize('a:1:{s:1:"a";a:1:{i:0;i:1;}}');
    $copy = $plain; $copy['a'][0] = 42; echo $plain['a'][0], "\n";
    unset($plain, $copy);
}
"#;
    let expected = "99:8:7|1\n".repeat(20);
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
