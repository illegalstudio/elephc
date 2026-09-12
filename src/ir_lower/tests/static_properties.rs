//! Purpose:
//! Pins executable shutdown ownership for emitted class static properties.
//!
//! Called from:
//! - The AST-to-EIR unit-test module through Rust's test harness.
//!
//! Key details:
//! - All five targets must skip uninitialized slots and detach inherited owners once.
//! - Destructor boundaries must finish remaining slots before propagating exceptions.

/// Every target retires inherited class-static owners before bounded shutdown release.
#[test]
fn class_static_shutdown_ownership_is_emitted_on_all_targets() {
    let source = r#"<?php
class ShutdownEmitterOwner {
    public static string $text = "";
    public static mixed $payload = null;
    public static string $uninitialized;
}
class ShutdownEmitterChild extends ShutdownEmitterOwner {}
ShutdownEmitterChild::$text = str_repeat("x", $argc);
ShutdownEmitterChild::$payload = ["key" => $argc];
echo isset(ShutdownEmitterChild::$uninitialized);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        for property in ["text", "payload", "uninitialized"] {
            let symbol = crate::names::static_property_symbol("ShutdownEmitterOwner", property);
            let comment = format!("epilogue cleanup static property {symbol}");
            assert_eq!(asm.matches(&comment).count(), 1, "{name}: {property}");
            let cleanup = asm.split_once(&comment).unwrap().1;
            let retired = cleanup.find("retire the static property").unwrap();
            let released = cleanup.find("__rt_cleanup_preserve_exception").unwrap();
            assert!(retired < released, "{name}: {property} must retire before destructor entry");
            let inherited = crate::names::static_property_symbol("ShutdownEmitterChild", property);
            assert!(!asm.contains(&format!("epilogue cleanup static property {inherited}")), "{name}");
        }
        let tail = asm.rsplit_once("retire the static property").unwrap().1;
        assert!(tail.find("__rt_cleanup_preserve_exception").unwrap() < tail.find("__rt_throw_current").unwrap(), "{name}");
    }
}
