//! Purpose:
//! Regressions for task 5f9dcd6d: packed writers and throwing capacity guards.
//!
//! Called from:
//! - The parent method-inlining EIR tests.
//!
//! Key details:
//! - Uses the executable sprite-batch example and checks every supported target.

use super::*;

/// Eight nested packed writes and a cold throw must lose their method-call overhead.
#[test]
fn sprite_batch_helpers_inline_on_every_target() {
    let source = include_str!("../../../examples/packed-sprite-batch/main.php");
    let throw_line = source.lines().position(|line| line.contains("throw new OverflowException")).unwrap() + 1;
    for target in TARGETS {
        let module = optimized(source, target);
        let add = method(&module, "SpriteBatch::add");
        let calls = called_methods(&module, add);
        assert!(!calls.iter().any(|name| matches!(name.as_str(), "vertex" | "reserve")),
            "{target}: sprite helpers survived: {calls:?}\n{}", crate::ir::print_function(add));
        let writes = placed(add).filter(|inst| inst.op == Op::PropSet && add.is_packed_field_access(inst)).count();
        assert_eq!(writes, 32, "{target}: all four vertices must be written in the caller");
        assert!(placed(add).any(|inst| inst.op == Op::ObjectNew
            && inst.span.is_some_and(|span| span.line as usize == throw_line)),
            "{target}: the cold exception must keep its construction line");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A user exception constructor and an extended builtin layout retain their real method frame.
#[test]
fn sprite_batch_guard_keeps_user_constructors_and_special_throwables_real() {
    let source = r#"<?php
class CustomOverflow extends RuntimeException {
    public function __construct() { parent::__construct('custom'); }
}
final class Guard {
    public function custom(bool $full): int {
        if ($full) { throw new CustomOverflow(); }
        return 1;
    }
    public function special(bool $full): int {
        if ($full) { throw new ErrorException('special', 0, 1); }
        return 2;
    }
}
function custom(Guard $guard, bool $full): int { return $guard->custom($full); }
function special(Guard $guard, bool $full): int { return $guard->special($full); }
echo custom(new Guard(), $argc > 3), special(new Guard(), $argc > 3);
"#;
    for target in TARGETS {
        let module = optimized(source, target);
        for name in ["custom", "special"] {
            assert!(called_methods(&module, function(&module, name)).iter().any(|call| call == name),
                "{target}: {name} must retain its call boundary");
        }
    }
}
