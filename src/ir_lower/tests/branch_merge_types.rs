//! Purpose:
//! Pins the representation of the hidden merge temporaries that `?:`, short `?:`, `??` and
//! `match` lower through.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - The merge temp is typed from the LOWERED branch values. Homogeneous branches must keep
//!   their native representation (no boxing); branches whose representations differ must merge
//!   into a boxed `mixed` (or a nullable union) instead of one branch's slot type.

use super::*;
use crate::ir::LocalKind;
use crate::types::PhpType;

/// Returns the codegen representations of every owned merge temp in function `name`.
fn merge_temp_reprs(source: &str, name: &str) -> Vec<PhpType> {
    let module = lower_source(source);
    let function = module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("function {name} not lowered"));
    function
        .locals
        .iter()
        .filter(|local| {
            local.kind == LocalKind::OwnedTemp
                && local.name.as_deref().is_some_and(|n| n.starts_with("__eir_tmp"))
        })
        .map(|local| local.php_type.codegen_repr())
        .collect()
}

#[test]
fn homogeneous_branch_merges_keep_native_representation() {
    let source = r#"<?php
class P { public int $v = 1; }
function ti(bool $c, int $a, int $b) { return $c ? $a : $b; }
function ts(bool $c, string $a, string $b) { return $c ? $a : $b; }
function to(bool $c, P $a, P $b) { return $c ? $a : $b; }
function si(int $a, int $b) { return $a ?: $b; }
function ci(?int $a, int $b) { return $a ?? $b; }
function cs(?string $a, string $b) { return $a ?? $b; }
function mi(int $k) { return match ($k) { 1 => 10, 2 => 20, default => 30 }; }
function ms(int $k) { return match ($k) { 1 => "a", default => "b" }; }
"#;
    let object = PhpType::Object("P".to_string());
    for (name, expected) in [
        ("ti", PhpType::Int),
        ("ts", PhpType::Str),
        ("to", object.clone()),
        ("si", PhpType::Int),
        ("ci", PhpType::Int),
        ("cs", PhpType::Str),
        ("mi", PhpType::Int),
        ("ms", PhpType::Str),
    ] {
        let reprs = merge_temp_reprs(source, name);
        assert!(!reprs.is_empty(), "{name}: expected a merge temp");
        assert!(
            reprs.iter().all(|repr| *repr == expected),
            "{name}: merge temps {reprs:?}, expected all {expected:?}"
        );
    }
}

#[test]
fn heterogeneous_branch_merges_box_instead_of_coercing() {
    let source = r#"<?php
class R { public $n = "s"; }
function gr(): mixed { return new R(); }
function uf(int $k): R|false { return $k > 9 ? false : new R(); }
function tm(bool $c) { $r = gr(); return $c ? 1 : $r->n; }
function tu(int $k) { $r = uf($k); return $r === false ? false : $r->n; }
function ib(bool $c) { return $c ? 42 : false; }
function sm(bool $c) { $r = gr(); return $r->n ?: 3; }
function cm(?int $a) { $r = gr(); return $a ?? $r->n; }
function mm(int $k) { $r = gr(); return match ($k) { 1 => 2, default => $r->n }; }
function co(?R $a, R $b) { return $a ?? $b; }
function so(?R $a, R $b) { return $a ?: $b; }
"#;
    // `?R` is a boxed `mixed` cell; narrowing its non-null arm to an `R` slot would store the box
    // pointer as an object, so these merges stay boxed.
    for name in ["tm", "tu", "ib", "sm", "cm", "mm", "co", "so"] {
        let reprs = merge_temp_reprs(source, name);
        assert!(
            reprs.contains(&PhpType::Mixed),
            "{name}: merge temps {reprs:?} must include a boxed mixed merge"
        );
        assert!(
            !reprs.iter().any(|repr| matches!(repr, PhpType::Int | PhpType::Bool)),
            "{name}: merge temps {reprs:?} must not coerce into a scalar slot"
        );
    }
}
