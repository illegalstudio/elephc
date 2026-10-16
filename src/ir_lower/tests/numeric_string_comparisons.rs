//! Purpose:
//! Pins boxed string equality and length-aware numeric classification across supported targets.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Mixed tags must select PHP equality rules rather than unconditional integer coercion.
//! - Typed and boxed strings share the numeric helper that checks the complete PHP byte length.

/// Every backend dispatches boxed equality and both numeric string paths to the shared helpers.
#[test]
fn review_numeric_string_helpers_are_selected_on_all_targets() {
    use crate::codegen::platform::Target;
    use std::path::Path;

    let source = r#"<?php
function reviewLoosePair(mixed $a, mixed $b): bool { return $a == $b; }
function reviewTypedNumeric(string $a): bool { return is_numeric($a); }
function reviewBoxedNumeric(mixed $a): bool { return is_numeric($a); }
echo reviewLoosePair("1e309", "1e310");
echo reviewTypedNumeric("2\0"), reviewBoxedNumeric("2\0");
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_mixed_loose_eq"), "{name}: {assembly}");
        assert_eq!(assembly.matches("__rt_str_numeric_ex").count(), 2, "{name}: {assembly}");
        assert!(!assembly.contains("__rt_str_to_number"), "{name}: {assembly}");
        assert!(!assembly.contains("__rt_mixed_cast_int"), "{name}: {assembly}");
    }
}
