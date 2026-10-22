//! Purpose:
//! Diagnostics for class members reached through a runtime receiver (`$class::X`,
//! `$class::$prop`, `$obj::m()`): every write through the read desugar is refused, since it
//! would land in a temporary.
//!
//! Called from:
//! - `cargo test --test error_tests dynamic_class_members`.
//!
//! Key details:
//! - The reads themselves are covered by `codegen::parser_expr` and
//!   `codegen::oop::dynamic_class_members`.

use super::*;

/// Verifies dynamic static-property writes are rejected with a targeted message.
#[test]
fn test_error_dynamic_static_property_write_is_rejected() {
    expect_error(
        "<?php class C { public static $n = 0; } $c = 'C'; $c::$n = 5;",
        "Writing a static property through a dynamic class",
    );
}

/// Verifies appends and keyed writes through `$class::$prop` are rejected clearly in statement
/// and expression position.
#[test]
fn test_error_dynamic_static_property_append_is_rejected() {
    for src in [
        "<?php class C { public static $t = []; } $c = 'C'; $c::$t[] = 'a';",
        "<?php class C { public static $t = []; } $c = 'C'; $x = ($c::$t[] = 'b');",
        "<?php class C { public static $t = []; } $c = 'C'; $f = fn($v) => $c::$t[] = $v;",
        "<?php class C { public static $t = []; } $c = 'C'; $c::$t['k'] = 'a';",
        "<?php class C { public static $t = []; } $c = 'C'; $x = ($c::$t['k'] = 'a');",
    ] {
        expect_error(src, "Writing a static property through a dynamic class");
    }
}

/// Verifies every other write context rooted in `$class::$prop` is rejected clearly.
#[test]
fn test_error_dynamic_static_property_write_contexts_are_rejected() {
    let head = "<?php class C { public static $p = ['n' => 1, 0 => [1]]; } $c = 'C';";
    for body in [
        "$c::$p['n']++;",
        "$c::$p['n']--;",
        "$c::$p[0][0]++;",
        "++$c::$p['n'];",
        "$x = $c::$p['n']++;",
        "$r = &$c::$p['n'];",
        "foreach ($c::$p as &$v) { $v = 9; }",
        "[$c::$p['n']] = [5];",
        "class D { public static $p = []; public function m() { $this::$p['n']++; } }",
    ] {
        expect_error(&format!("{head} {body}"), "Writing a static property through a dynamic class");
    }
}

/// Verifies a builtin by-reference parameter rejects a `$class::$prop[k]` / `$class::K[k]`
/// argument (it would write into a temporary), like a call-result argument.
#[test]
fn test_error_dynamic_member_to_builtin_by_ref_parameter_is_rejected() {
    let head = "<?php class C { public static $p = ['n' => 1, 'a' => [1, 2, 3]]; const K = ['a' => [1]]; } $c = 'C';";
    for body in [
        "settype($c::$p['n'], 'string');",
        "array_splice($c::$p['a'], 0, 1);",
        "array_splice($c::K['a'], 0, 1);",
        "$o = new C; settype($o::$p['n'], 'string');",
    ] {
        expect_error(&format!("{head} {body}"), "Writing a static property through a dynamic class");
    }
}

/// Verifies a `foreach` value or key target rooted in `$class::$prop` is rejected: the loop
/// would store into the temporary the read returns and leave the static property unchanged.
#[test]
fn test_error_dynamic_static_property_foreach_targets_are_rejected() {
    let head = "<?php class C { public static $p = [0, 'k' => 1]; public static $o; } $c = 'C';";
    for body in [
        "foreach ([7] as $c::$p[0]) {}",
        "foreach (['a' => 7] as $c::$p['k'] => $v) {}",
        "foreach ([7] as $c::$o->x) {}",
    ] {
        expect_error(&format!("{head} {body}"), "Writing a static property through a dynamic class");
    }
}
