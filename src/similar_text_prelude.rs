//! Purpose:
//! PHP's `similar_text()` — the recursive longest-common-substring count and the percentage it
//! optionally writes back — implemented in elephc-PHP. The builtin keeps its registry contract
//! and lowers to the functions declared here, so neither target needs assembly for it.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, after
//!   include resolution and before name resolution.
//! - `crate::builtins::string::similar_text`, whose EIR lowering emits a direct call to
//!   `__elephc_similar_text` or `__elephc_similar_text_pct`.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT ASSEMBLY. `similar_text()` was absent entirely — `Undefined function`
//!   — and php's algorithm is a doubly-nested scan that RECURSES on both sides of the longest
//!   match it finds. Written in PHP it is correct on both targets at once and reads against
//!   `ext/standard/string.c`'s `php_similar_str` / `php_similar_char` line by line; written twice
//!   in assembly it would be two chances to get a recursion wrong.
//! - PAY-FOR-USE. Injected only when the program references `similar_text`, so a program that
//!   never calls it carries none of this.
//! - MEASURED, NOT GUESSED. The algorithm was differed against `php -n` 8.5.6's own builtin over
//!   576 string pairs — empty strings, equal strings, disjoint strings, repeated runs
//!   (`mississippi`/`missouri`), reversals (`abcdefghij`/`jihgfedcba`), embedded NULs — with zero
//!   mismatches in either the count or the percentage.
//! - THE PERCENTAGE IS `sim * 2 / (len1 + len2) * 100`, and both strings empty is the one case
//!   that would divide by zero: php answers `int(0)` and `float(0)`, so the guard answers the
//!   same rather than producing NAN.
//! - `$percent` IS ITS OWN ENTRY POINT. A by-reference parameter cannot be conditionally present,
//!   so the two arities are two functions, which is the shape `crate::scanf_prelude` already uses
//!   for the same reason.
//! - AND IT IS DECLARED `mixed`, not left untyped. An untyped by-reference parameter takes its
//!   type from the call site, and a prelude function is resolved before any call site is seen, so
//!   it falls back to the `int` placeholder: MEASURED, the caller handed over a Mixed cell and
//!   read back its own initial `0.0` for two calls and uninitialized memory
//!   (`float(2.125687991E-314)`) for a third. The declaration is what pins the two together —
//!   the same trap `crate::scanf_prelude` documents for its `$vars` wrappers.

mod detect;

/// The elephc-PHP `similar_text()` engine.
///
/// `__elephc_similar_char` is php's `php_similar_char`: find the longest common substring, then
/// add the counts of the two REMAINDERS on either side of it. `$pos1 > 0 && $pos2 > 0` guards the
/// left recursion because php only recurses into a prefix that exists on BOTH sides, and the
/// right guard is the same test for the suffix.
pub(crate) const SIMILAR_TEXT_PRELUDE_SRC: &str = r#"<?php

function __elephc_similar_char(string $t1, string $t2): int {
    $len1 = strlen($t1);
    $len2 = strlen($t2);
    $max = 0;
    $pos1 = 0;
    $pos2 = 0;
    for ($p = 0; $p < $len1; $p++) {
        for ($q = 0; $q < $len2; $q++) {
            $l = 0;
            while ($p + $l < $len1 && $q + $l < $len2 && $t1[$p + $l] === $t2[$q + $l]) {
                $l++;
            }
            if ($l > $max) {
                $max = $l;
                $pos1 = $p;
                $pos2 = $q;
            }
        }
    }
    $sum = $max;
    if ($sum > 0) {
        if ($pos1 > 0 && $pos2 > 0) {
            $sum += __elephc_similar_char(substr($t1, 0, $pos1), substr($t2, 0, $pos2));
        }
        if ($pos1 + $max < $len1 && $pos2 + $max < $len2) {
            $sum += __elephc_similar_char(substr($t1, $pos1 + $max), substr($t2, $pos2 + $max));
        }
    }
    return $sum;
}

function __elephc_similar_text(string $string1, string $string2): int {
    return __elephc_similar_char($string1, $string2);
}

function __elephc_similar_text_pct(string $string1, string $string2, mixed &$percent): int {
    $sim = __elephc_similar_char($string1, $string2);
    $total = strlen($string1) + strlen($string2);
    if ($total === 0) {
        $zero = 0.0;
        $percent = $zero;
        return $sim;
    }
    $pct = ($sim * 2.0) / $total * 100.0;
    $percent = $pct;
    return $sim;
}
"#;

/// The reachability group these declarations belong to.
///
/// They are named only by the lowering of `similar_text()`, never by PHP source, so reachability
/// has no edge to follow to them and the group has to be forced wherever that pass runs. It is
/// not a hole in pay-for-use: the group exists only when the prelude was injected, which happens
/// only when the program references the builtin.
pub const PRELUDE_GROUP_ID: &str = "similar_text";

/// Injects the `similar_text()` engine when the program references it, leaving every other
/// program untouched.
///
/// There is no user-declaration escape hatch, unlike `dir()`: `similar_text` is a registry
/// builtin, so PHP itself refuses to redeclare it and no program can own the name. The prelude
/// carries only declarations, so prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(
    program: crate::parser::ast::Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> crate::parser::ast::Program {
    if !detect::program_references_similar_text(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(SIMILAR_TEXT_PRELUDE_SRC)
        .expect("similar_text prelude must tokenize");
    let mut combined =
        crate::parser::parse_internal(&tokens).expect("similar_text prelude must parse");
    inventory.record_program(PRELUDE_GROUP_ID, &combined);
    combined.extend(program);
    combined
}
