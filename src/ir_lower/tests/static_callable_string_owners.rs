//! Purpose:
//! Pins late-bound static descriptor result normalization on all supported targets.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Result ownership follows the called class, not just the lexical implementation.
//! - String pair registers must survive the ownership selector.

/// Every ABI selects owned static overrides without overwriting the returned string pair.
#[test]
fn late_static_descriptor_string_ownership_uses_the_called_class_on_all_targets() {
    let source = r#"<?php
class StaticStringOwner {
    public static function render(string $value): string { return "owned" . $value; }
    public static function callback(): callable { return static::render(...); }
}
class StaticStringBorrower extends StaticStringOwner {
    public static function render(string $value): string { return $value; }
}
$callbacks = [StaticStringOwner::callback(), StaticStringBorrower::callback()];
foreach ($callbacks as $callback) { echo $callback("arg"); }
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let owner_id = module.class_infos["StaticStringOwner"].class_id;
        let borrower_id = module.class_infos["StaticStringBorrower"].class_id;
        let (expected_owner, unexpected_borrower, comparison) = if target == "linux-x86_64" {
            (format!("mov rcx, {owner_id}"), format!("mov rcx, {borrower_id}"), "cmp r10, rcx")
        } else {
            (format!("mov x11, #{owner_id}"), format!("mov x11, #{borrower_id}"), "cmp x10, x11")
        };
        let selectors: Vec<_> = asm.split("normalize late-bound static descriptor string ownership")
            .skip(1).map(|tail| tail.split("ret\n").next().unwrap()).collect();
        assert!(!selectors.is_empty(), "{target}: the late-bound factory must normalize string results");
        for selector in selectors {
            assert!(selector.contains(&expected_owner), "{target}: retain the heap-returning implementation");
            assert!(!selector.contains(&unexpected_borrower), "{target}: borrowed overrides still need persistence");
            assert!(selector.contains(comparison), "{target}: preserve string-result registers");
            assert_eq!(selector.matches("__rt_str_persist").count(), 1, "{target}: one borrowed-result copy");
        }
    }
}
