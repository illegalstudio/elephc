use super::*;

#[test]
fn test_chained_closure_call() {
    let out = compile_and_run(
        "<?php $f = function() { return function() { return 99; }; }; echo $f()();",
    );
    assert_eq!(out, "99");
}

// --- do...while ---
