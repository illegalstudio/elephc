//! Purpose:
//! Verifies shared native return epilogues preserve path-specific ownership and destruction.
//!
//! Called from:
//! - Executable runtime ownership CI on every supported host target.
//!
//! Key details:
//! - Returning different locals must transfer exactly the selected owner.
//! - Equivalent scalar returns keep the same cleanup order with EIR optimization on or off.

use crate::support::*;

/// Shared cleanup releases siblings before return and leaves each returned local alive for its caller.
#[test]
fn test_shared_native_return_epilogues_preserve_local_transfers() {
    let source = r#"<?php
class SharedReturnOwner {
    public function __construct(public string $name) {}
    public function __destruct() { echo 'destroy:', $this->name, '|'; }
}
function chooseSharedNativeReturn(int $choice): mixed {
    $first = new SharedReturnOwner('first');
    $second = new SharedReturnOwner('second');
    if ($choice === 0) { return $first; }
    if ($choice === 1) { return $second; }
    if ($choice === 2) { return 11; }
    return 22;
}
foreach ([0, 1, 2, 3] as $choice) {
    $result = chooseSharedNativeReturn($choice);
    echo is_object($result) ? $result->name : $result, '|';
    unset($result); echo '#';
}
"#;
    let expected = concat!(
        "destroy:second|first|destroy:first|#",
        "destroy:first|second|destroy:second|#",
        "destroy:first|destroy:second|11|#",
        "destroy:first|destroy:second|22|#",
    );
    for optimize in [true, false] {
        let output = if optimize {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(output.success, "ir-opt={optimize}: stdout={} stderr={}", output.stdout, output.stderr);
        assert_eq!(output.stdout, expected, "ir-opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    }
}
