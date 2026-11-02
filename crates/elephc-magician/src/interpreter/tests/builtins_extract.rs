//! Purpose:
//! Interpreter tests for the `extract()` builtin: return-count semantics, the default
//! `EXTR_OVERWRITE` mode, `EXTR_SKIP`, the `EXTR_PREFIX_*`/`EXTR_IF_EXISTS` family with their
//! `$prefix` argument, insertion-order-dependent prefix collisions, `EXTR_REFS` real reference
//! semantics, and the catchable errors `extract()` raises for a non-array argument or an invalid
//! `$flags` value.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_extract`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's own output on the same fragment, measured via the
//!   `$R/gate.php` differential probe harness before this file was written.
//! - Flags are passed as raw integers (`1` for `EXTR_SKIP`, `256` for `EXTR_REFS`, ...) rather
//!   than the `EXTR_*` constant names: those names are a separate, later commit, and this file
//!   must stand on its own.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse extract() fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute extract() fragment");
    values.output.clone()
}

/// Verifies the basic shape: every valid string key becomes a variable, and the return value is
/// the number of variables actually extracted.
///
/// `php -n` 8.5.6 prints `extract:2:1:two`.
#[test]
fn extract_binds_every_valid_key_and_returns_the_count() {
    assert_eq!(
        out(br#"$src = ['alpha' => 1, 'beta' => 'two'];
$n = extract($src);
echo 'extract:', $n, ':', $alpha, ':', $beta;"#),
        "extract:2:1:two",
    );
}

/// Verifies the default mode (`EXTR_OVERWRITE`, flags omitted) overwrites an existing variable,
/// and `EXTR_SKIP` (`1`) leaves an existing variable untouched while still extracting a new one.
///
/// `php -n` 8.5.6 prints `skip:1:kept:3` then `over:1:new2`.
#[test]
fn extract_skip_leaves_an_existing_variable_and_overwrite_replaces_it() {
    assert_eq!(
        out(br#"$alpha = 'kept';
$n = extract(['alpha' => 'new', 'gamma' => 3], 1);
echo 'skip:', $n, ':', $alpha, ':', $gamma, "\n";
$m = extract(['alpha' => 'new2'], 0);
echo 'over:', $m, ':', $alpha;"#),
        "skip:1:kept:3\nover:1:new2",
    );
}

/// Verifies a numeric key and a syntactically invalid key (contains a space) are silently
/// skipped -- not counted -- when no prefix mode can repair them, while a valid key still binds.
///
/// `php -n` 8.5.6 prints `count=1 ok=fine`.
#[test]
fn extract_skips_invalid_and_numeric_keys_without_a_prefix_mode() {
    assert_eq!(
        out(br#"$n = extract([0 => "zero", "1abc" => "onebc", "ok" => "fine", "with space" => "sp"]);
echo "count=", $n, " ok=", $ok ?? "unset";"#),
        "count=1 ok=fine",
    );
}

/// Verifies `EXTR_PREFIX_ALL` (`3`) prefixes EVERY key, even one that is already a valid
/// variable name, and prefixes a numeric key that could not otherwise extract.
///
/// `php -n` 8.5.6 prints `count=2 pre_0=num pre_valid=v`.
#[test]
fn extract_prefix_all_prefixes_every_key_including_already_valid_names() {
    assert_eq!(
        out(br#"$n = extract(["0" => "num", "valid" => "v"], 3, "pre");
echo "count=", $n, " pre_0=", $pre_0 ?? "unset", " pre_valid=", $pre_valid ?? "unset";"#),
        "count=2 pre_0=num pre_valid=v",
    );
}

/// Verifies `EXTR_PREFIX_ALL` reprefixes a key that is ALREADY the target name another key's
/// prefixing would produce: key `"x"` becomes `pre_x`, and key `"pre_x"` becomes `pre_pre_x`, so
/// the two never collide regardless of insertion order.
///
/// `php -n` 8.5.6 prints `count=2 pre_x=first pre_pre_x=second`.
#[test]
fn extract_prefix_all_never_collides_with_its_own_prefixed_output() {
    assert_eq!(
        out(br#"$n = extract(["x" => "first", "pre_x" => "second"], 3, "pre");
echo "count=", $n, " pre_x=", $pre_x ?? "unset", " pre_pre_x=", $pre_pre_x ?? "unset";"#),
        "count=2 pre_x=first pre_pre_x=second",
    );
}

/// Verifies `EXTR_PREFIX_INVALID` (`4`) is where a genuine, insertion-order-dependent collision
/// happens: numeric key `"0"` is invalid so it prefixes to `pre_0`, while a literal key
/// `"pre_0"` is already valid so it extracts UNPREFIXED as `pre_0` too -- the two keys target the
/// SAME variable, and whichever comes later in the array wins, though the return count still
/// includes both successful extractions.
///
/// `php -n` 8.5.6 prints `order1:2:direct` then `order2:2:from-zero`.
#[test]
fn extract_prefix_invalid_collision_is_insertion_order_dependent() {
    assert_eq!(
        out(br#"$n1 = extract(["0" => "from-zero", "pre_0" => "direct"], 4, "pre");
echo "order1:", $n1, ":", $pre_0, "\n";
$n2 = extract(["pre_0" => "direct", "0" => "from-zero"], 4, "pre");
echo "order2:", $n2, ":", $pre_0;"#),
        "order1:2:direct\norder2:2:from-zero",
    );
}

/// Verifies `EXTR_PREFIX_SAME` (`2`) prefixes only the key that COLLIDES with an existing
/// variable -- `"same"` becomes `pre_same` because `$same` already exists, but `"fresh"`
/// extracts UNPREFIXED as plain `$fresh` because it does not -- so both count, `$same` itself is
/// left untouched, and only `$pre_same` (never `$pre_fresh`) is created. Also verifies
/// `EXTR_PREFIX_IF_EXISTS` (`5`) extracts NOTHING for a name that does not already exist, and
/// `EXTR_IF_EXISTS` (`6`) extracts unprefixed only when the name already exists.
///
/// `php -n` 8.5.6 prints `same:2:kept:new:unset` then `ifexists:0:kept` then `plain:1:new2`.
#[test]
fn extract_prefix_same_and_if_exists_family_match_php() {
    assert_eq!(
        out(br#"$same = "kept";
$n1 = extract(["same" => "new", "fresh" => "ignored"], 2, "pre");
echo "same:", $n1, ":", $same, ":", $pre_same ?? "unset", ":", $pre_fresh ?? "unset", "\n";
$n2 = extract(["missing" => "new"], 5, "pre");
echo "ifexists:", $n2, ":", $same, "\n";
$n3 = extract(["same" => "new2"], 6);
echo "plain:", $n3, ":", $same;"#),
        "same:2:kept:new:unset\nifexists:0:kept\nplain:1:new2",
    );
}

/// Verifies `EXTR_REFS` (`256`) binds a REAL reference to the source array's own storage, not a
/// copy: writing the extracted local variable afterward is observable through the source array,
/// and combining it with `EXTR_SKIP` (`256 | 1 = 257`) still applies the collision policy while
/// keeping the by-reference bind for every key that DOES extract.
///
/// `php -n` 8.5.6 prints `refcount=2:src_a=100` then `refskip=1:a=kept:src_b=999`.
#[test]
fn extract_refs_binds_a_live_reference_into_the_source_array() {
    assert_eq!(
        out(br#"function refCase() {
    $src = ["a" => 1, "b" => 2];
    $n = extract($src, 256);
    $a = 100;
    echo "refcount=", $n, ":src_a=", $src["a"], "\n";
}
refCase();
function refSkipCase() {
    $a = "kept";
    $src = ["a" => 1, "b" => 2];
    $n = extract($src, 257);
    $b = 999;
    echo "refskip=", $n, ":a=", $a, ":src_b=", $src["b"];
}
refSkipCase();"#),
        "refcount=2:src_a=100\nrefskip=1:a=kept:src_b=999",
    );
}

/// Verifies `EXTR_REFS` on a source with no addressable storage (an array literal, not a
/// variable) falls back to an ordinary value bind instead of erroring -- php raises no warning
/// either.
///
/// `php -n` 8.5.6 prints `litref=1:z=42`.
#[test]
fn extract_refs_on_a_non_variable_source_still_binds_by_value() {
    assert_eq!(
        out(br#"$n = extract(["z" => 42], 256);
echo "litref=", $n, ":z=", $z ?? "unset";"#),
        "litref=1:z=42",
    );
}

/// Verifies a non-array first argument is a catchable `TypeError` naming the given type the way
/// php names it for an internal-function argument, and that no variable is created.
///
/// `php -n` 8.5.6 throws `extract(): Argument #1 ($array) must be of type array, string given`
/// for a string, `..., int given` for an int and `..., null given` for null.
#[test]
fn extract_rejects_a_non_array_argument_with_a_catchable_type_error() {
    assert_eq!(
        out(br#"try {
    extract("not-an-array");
} catch (\TypeError $e) {
    echo "caught1:", $e->getMessage(), "\n";
}
try {
    extract(5);
} catch (\TypeError $e) {
    echo "caught2:", $e->getMessage(), "\n";
}
try {
    extract(null);
} catch (\TypeError $e) {
    echo "caught3:", $e->getMessage();
}
echo ":a=", $a ?? "unset";"#),
        "caught1:extract(): Argument #1 ($array) must be of type array, string given\n\
caught2:extract(): Argument #1 ($array) must be of type array, int given\n\
caught3:extract(): Argument #1 ($array) must be of type array, null given:a=unset",
    );
}

/// Verifies an out-of-range `$flags` value is a catchable `ValueError`, not a silent no-op or a
/// fatal, and that no variable is created.
///
/// `php -n` 8.5.6 throws `extract(): Argument #2 ($flags) must be a valid extract type`.
#[test]
fn extract_rejects_an_invalid_flags_value_with_a_catchable_value_error() {
    assert_eq!(
        out(br#"try {
    extract(["a" => 1], 999);
} catch (\ValueError $e) {
    echo "caught:", $e->getMessage();
}
echo ":a=", $a ?? "unset";"#),
        "caught:extract(): Argument #2 ($flags) must be a valid extract type:a=unset",
    );
}
