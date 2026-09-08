//! Purpose:
//! Verifies that generated PHP debug ranges exclude separately emitted native helpers.
//!
//! Called from:
//! - The AST-to-EIR test module through Rust's test harness.
//!
//! Key details:
//! - Linux DWARF range subtraction requires both labels to occupy the same ELF section.
//! - Generator source ranges describe the body, not its constructor or coroutine wrapper.

/// Functions, methods, and generator bodies delimit their debug ranges before cleanup sections.
#[test]
fn native_debug_ranges_stay_in_their_function_sections_on_all_targets() {
    let source = r#"<?php
        function debug_value(int $value): int { return $value + 1; }
        function debug_items(int $value): Generator { yield $value; }
        class DebugRangeValue {
            public function read(int $value): int { return $value + 2; }
            public function items(int $value): Generator { yield $value + 3; }
        }
        $object = new DebugRangeValue();
        echo debug_value($argc), $object->read($argc);
        foreach (debug_items($argc) as $value) { echo $value; }
        foreach ($object->items($argc) as $value) { echo $value; }
    "#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let mut open = None;
        let mut section = "";
        let mut entry_section = None;
        let mut ranges = 0;
        for line in asm.lines() {
            if line.starts_with(".section ") {
                section = line;
            }
            if line.contains("@fn ") {
                assert!(open.is_none(), "{name}: source ranges cannot nest");
                open = line.split_whitespace().find_map(|part| part.strip_prefix("symbol="));
                assert!(open.is_some(), "{name}: function range has an entry symbol");
                entry_section = None;
            } else if line.contains("@endfn") {
                let symbol = open.take().expect("each end marker closes exactly one range");
                let start = entry_section.take().expect("the entry label exists inside its range");
                if target.platform == crate::codegen::platform::Platform::Linux {
                    assert_eq!(start, section, "{name}: DWARF range for {symbol} must not span ELF sections");
                }
                ranges += 1;
            } else if line.ends_with("__cdylib_exception_cleanup:") {
                assert!(open.is_none(), "{name}: cleanup helpers cannot extend a PHP source range");
            } else if open.is_some() && line.strip_suffix(':') == open {
                entry_section = Some(section);
            }
        }
        assert!(open.is_none(), "{name}: all source ranges must close");
        assert!(asm.contains("@fn name=debug_items symbol=_fn_debug_items__genbody"), "{name}: generator body has debug information");
        assert!(ranges >= 5, "{name}: user functions, methods, generators, and main have ranges");
    }
}
