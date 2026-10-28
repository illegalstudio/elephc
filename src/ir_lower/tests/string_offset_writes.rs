//! Purpose:
//! Checks direct and boxed string offset writes across every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit test harness.
//!
//! Key details:
//! - Write-key conversion is shared with nested writes.
//! - Fresh write results remain releasable under the current string ownership contract.

/// Lowers runtime operands and checks the generated target assembly and ownership metadata.
fn verify_string_offset_write_target(target: &str) {
    let source = r#"<?php
function direct(string $s, int $key, string $value): string {
    $s[$key] = $value;
    return $s;
}
function boxed(mixed $s, mixed $key, string $value): mixed {
    return ($s[$key] = $value);
}
$nested = ['abc']; $nested[0][1] = 'N';
$typed = new SplFixedArray(1); $typed[0] = 'typed'; echo $typed[0];
echo direct(str_repeat('a', $argc + 2), $argc, 'D');
echo boxed('abc', $argc, 'B'), json_encode($nested);
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let direct = module.functions.iter().find(|function| function.name == "direct").unwrap();
    let write = direct.instructions.iter().find(|inst| inst.immediate == Some(
        crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::StringOffsetSet),
    )).expect("direct write must retain its typed runtime boundary");
    assert!(!write.string_release_is_noop(), "{target}: updated strings own releasable storage");
    assert!(direct.instructions.iter().any(|inst| inst.immediate == Some(
        crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::StringWriteOffset),
    )), "{target}: direct writes share nested offset conversion");
    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    assert!(asm.contains("__rt_str_offset_set"), "{target}: missing string write call");
}

/// Keeps each supported target independently selectable by the focused test filter.
macro_rules! string_write_target_test {
    ($name:ident, $target:literal) => {
        /// Verifies string write lowering on this supported target.
        #[test]
        fn $name() { verify_string_offset_write_target($target); }
    };
}

string_write_target_test!(string_offset_write_target_macos, "macos-aarch64");
string_write_target_test!(string_offset_write_target_ios, "ios-arm64");
string_write_target_test!(string_offset_write_target_ios_sim, "ios-sim-arm64");
string_write_target_test!(string_offset_write_target_linux_arm, "linux-aarch64");
string_write_target_test!(string_offset_write_target_linux_x86, "linux-x86_64");
