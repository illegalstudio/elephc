//! Purpose:
//! Pins declared-array and callable membership lowering across every supported target.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Dynamic haystacks require a checked runtime traversal, never a raw array reinterpretation.

/// Mixed needles, boxed arrays, floating needles and opaque callables all select a checked scan.
#[test]
fn boxed_array_membership_lowers_on_every_target() {
    let source = r#"<?php
function memberOnTarget(mixed $needle, array $items, bool $strict): bool {
    return in_array($needle, $items, $strict);
}
function floatMemberOnTarget(float $needle, array $items): bool {
    return in_array($needle, $items, true);
}
function stringMemberOnTarget(string $needle, array $items): bool {
    return in_array($needle, $items, true);
}
function opaqueMemberOnTarget(callable $probe, array $items): bool {
    return $probe(1, $items, true);
}
echo memberOnTarget($argc, ['one' => 1], $argc > 0),
    floatMemberOnTarget(1.0, [1.0]), stringMemberOnTarget('one', ['one']),
    opaqueMemberOnTarget(in_array(...), [1]);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_in_array_boxed"), "{target}");
        assert!(asm.contains("in_array(): Argument #2 ($haystack) must be of type array"), "{target}");
    }
}
