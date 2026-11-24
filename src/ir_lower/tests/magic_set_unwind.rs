//! Purpose:
//! Verifies the magic `__set` caught path enters the exception unwinder with a call frame.
//!
//! Called from:
//! - The AST-to-EIR unit suite through `crate::ir_lower::tests`.
//!
//! Key details:
//! - The caught path releases its guard frame and holds an aligned stack. A tail jump into
//!   `__rt_throw_current` leaves SysV eight bytes off, so destructors that call a Rust bridge
//!   fault on x86_64. Every target must publish the call linkage instead.

use crate::codegen::platform::Target;
use std::path::Path;

/// Compiles a throwing magic setter and checks the catch edge calls the unwinder.
#[test]
fn magic_set_catch_path_calls_the_unwinder_on_every_target() {
    let source = r#"<?php
class Sink {
    public function __set(string $name, mixed $value): void {
        throw new Exception("x");
    }
}
$sink = new Sink();
try {
    $sink->value = 1;
} catch (Throwable $error) {
    echo $error->getMessage();
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(target).expect("target");
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            target,
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .expect("magic-set assembly");
        let label = assembly
            .find("runtime_magic_set_caught")
            .unwrap_or_else(|| panic!("{}: no magic-set catch edge", target.as_str()));
        let tail = &assembly[label..];
        let throw = tail
            .find("__rt_throw_current")
            .unwrap_or_else(|| panic!("{}: catch edge never reaches the unwinder", target.as_str()));
        let before = tail[..throw].trim_end();
        let call = if target.as_str() == "linux-x86_64" { "call" } else { "bl" };
        assert!(
            before.ends_with(call),
            "{}: magic-set catch must call the unwinder, not tail-jump into it:\n{}",
            target.as_str(),
            &tail[..throw],
        );
    }
}
