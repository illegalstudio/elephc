//! Purpose:
//! Pins the refusal of a by-reference iteration or element reference through a PHP variable
//! that shares a reference set with a typed-element array.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Making the entries referenceable re-lays the shared cell out as a hash of boxed Mixed
//!   entries. The other names of the reference set keep their packed typed layout, so the
//!   shape is refused instead of crashing (it used to SIGSEGV) or reading hash internals.
//! - A mixed-element list keeps lowering: the other names dispatch on the runtime layout.

use crate::codegen::platform::Target;
use crate::ir_lower::LoweringError;
use std::path::Path;

/// Lowers `source` for the host target and returns its refusal message, or `None` if it lowered.
fn lowering_refusal(source: &str) -> Option<String> {
    match super::try_lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::detect_host(),
    ) {
        Ok(_) => None,
        Err(LoweringError::Unsupported(error)) => Some(error.message),
        Err(other) => panic!("expected an unsupported-shape refusal, got {other:?}"),
    }
}

/// `$y = &$x[0]`, `foreach ($x as &$v)` and a `use (&$list)` capture over a typed list alias
/// are refused with the explicit reference-set message.
#[test]
fn typed_list_reference_alias_promotion_is_refused() {
    for source in [
        "<?php $res = [1]; $x = &$res; $y = &$x[0]; $y = 5; echo $res[0];",
        "<?php $res = [1, 2]; $x = &$res; foreach ($x as &$v) { $v++; } echo $res[0];",
        "<?php $h = ['a' => 1]; $x = &$h; foreach ($x as &$v) { $v++; } echo $h['a'];",
        "<?php $l = [1]; $f = function () use (&$l) { foreach ($l as &$v) { $v++; } }; $f();",
    ] {
        let message = lowering_refusal(source)
            .unwrap_or_else(|| panic!("expected a refusal for {source}"));
        assert!(
            message.contains("is part of a PHP reference set"),
            "{source}: {message}"
        );
    }
}

/// A mixed-element list, a plain typed local and a synthetic property alias still lower.
#[test]
fn mixed_list_reference_alias_promotion_still_lowers() {
    for source in [
        "<?php $res = [1, 'a']; $x = &$res; $y = &$x[0]; foreach ($x as &$v) {} echo $res[0];",
        "<?php $res = [1, 2]; $y = &$res[0]; foreach ($res as &$v) { $v++; } echo $res[0];",
        "<?php class C { public array $a = [1]; } $c = new C(); foreach ($c->a as &$v) { $v++; }",
    ] {
        assert_eq!(lowering_refusal(source), None, "{source}");
    }
}
