//! Purpose:
//! Verifies target-aware guards in specialized mbstring count lowering.
//!
//! Called from:
//! - The AST-to-EIR integration unit suite.
//!
//! Key details:
//! - Every supported target emits the live request guard and count failure branch.
//! - A UTF-8 check answered by the native validator leaves the bridge runtime disabled.

/// Length-only regex specialization keeps state and invalid-result guards on every target.
#[test]
fn mb_fast_semantics_count_guards_cover_all_targets() {
    let source = r#"<?php
$s = $argc > 0 ? "a@b" : "";
echo count(mb_split("@", $s));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        crate::ir_passes::optimize_module(&mut module);
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("elephc_mbstring_regex_ascii_defaults_v1"), "{target}: live state guard");
        assert!(asm.contains("mb_split_count_array"), "{target}: false-result count guard");
    }
}

/// Leaves the bridge runtime out when every mbstring call is a native UTF-8 validation.
#[test]
fn native_utf8_check_encoding_leaves_mbstring_runtime_disabled() {
    for source in [
        "<?php function valid(string $text): bool { return mb_check_encoding($text, 'UTF-8'); } var_dump(valid('Caffè'));",
        "<?php var_dump(mb_check_encoding('Caffè', 'utf8'));",
        "<?php $check = Closure::fromCallable(function (): void { var_dump(mb_check_encoding('Caffè', 'UTF-8')); }); $check();",
    ] {
        let features = super::lower_source(source).required_runtime_features;
        assert!(!features.mbstring && !features.eval_bridge, "{source}: {features:?}");
    }
    for source in [
        "<?php function valid(mixed $value): bool { return mb_check_encoding($value, 'UTF-8'); } var_dump(valid(['a']));",
        "<?php function valid(?string $text): bool { return mb_check_encoding($text, 'UTF-8'); } var_dump(valid(null));",
        "<?php function valid(string $text, string $encoding): bool { return mb_check_encoding($text, $encoding); } var_dump(valid('a', 'UTF-8'));",
        "<?php var_dump(mb_check_encoding('a', 'ISO-8859-1'));",
        "<?php var_dump(mb_check_encoding('a'));",
        "<?php $check = mb_check_encoding(...); var_dump($check('a', 'UTF-8'));",
        "<?php var_dump(mb_check_encoding('a', 'UTF-8'), mb_strlen('a'));",
    ] {
        assert!(super::lower_source(source).required_runtime_features.mbstring, "{source}");
    }
}

/// Emits the native UTF-8 check without any bridge reference on every supported target.
#[test]
fn native_utf8_check_encoding_links_no_bridge_on_any_target() {
    let source = "<?php function valid(string $text): bool { return mb_check_encoding($text, 'UTF-8'); }\n\
        var_dump(valid($argc > 5 ? 'x' : \"\\xC3\\x28\"));";
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let parsed = crate::codegen::platform::Target::parse(target).unwrap();
        let mut module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), parsed,
        );
        crate::ir_passes::optimize_module(&mut module);
        assert!(!module.required_runtime_features.mbstring, "{target}: bridge runtime enabled");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("_rt_mb_strlen_utf8"), "{target}: native validator call");
        assert!(!asm.contains("rt_mbstring") && !asm.contains("elephc_mbstring"),
            "{target}: user code references the mbstring runtime");
        let runtime = crate::codegen::generate_runtime_with_features(8 << 20, parsed, module.required_runtime_features);
        assert!(runtime.contains("_rt_mb_strlen_utf8"), "{target}: runtime lacks the validator");
        assert!(!runtime.contains("elephc_mbstring_"), "{target}: runtime references the bridge");
    }
}
