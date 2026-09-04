//! Purpose:
//! Interpreter tests for the array-taking builtin `TypeError` family.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Every expected message is quoted from `php -n` 8.5.6, including the variadic
//!   spellings that carry no `($name)` segment.

use super::super::*;
use super::support::*;

/// Verifies eval `count()` throws PHP's `Countable|array` TypeError for non-countables.
#[test]
fn execute_program_count_rejects_non_countable_with_php_type_error() {
    let program = parse_fragment(
        br#"class EvalCountableSeven implements Countable { public function count(): int { return 7; } }
$false = false;
try { count($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { count(null); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { count("s"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { count(new stdClass()); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|" . count([1, 2]) . "|" . count(new EvalCountableSeven());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "count(): Argument #1 ($value) must be of type Countable|array, false given",
            "|count(): Argument #1 ($value) must be of type Countable|array, null given",
            "|count(): Argument #1 ($value) must be of type Countable|array, string given",
            "|count(): Argument #1 ($value) must be of type Countable|array, stdClass given",
            "|2|7",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `in_array()` names the offending VALUE's PHP type, not just its tag.
#[test]
fn execute_program_in_array_rejects_non_array_haystack_with_php_type_error() {
    let program = parse_fragment(
        br#"$false = false;
$true = true;
try { in_array("x", $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { in_array("x", $true); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { in_array("x", null); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { in_array("x", 3); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { in_array("x", 3.5); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { in_array("x", "s"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
return in_array(2, [1, 2, 3]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "in_array(): Argument #2 ($haystack) must be of type array, false given",
            "|in_array(): Argument #2 ($haystack) must be of type array, true given",
            "|in_array(): Argument #2 ($haystack) must be of type array, null given",
            "|in_array(): Argument #2 ($haystack) must be of type array, int given",
            "|in_array(): Argument #2 ($haystack) must be of type array, float given",
            "|in_array(): Argument #2 ($haystack) must be of type array, string given",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a fully variadic builtin drops the `($name)` segment on EVERY argument.
///
/// `array_merge()` is the family's odd member: `array_diff()` names its first argument
/// `$array` and leaves only the tail unnamed, while `array_merge()` names none of them.
#[test]
fn execute_program_variadic_array_builtins_omit_the_parameter_name() {
    let program = parse_fragment(
        br#"$false = false;
try { array_merge($false, []); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_merge([], $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_diff($false, []); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_diff([], $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_intersect_key([], $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_map("strlen", $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "array_merge(): Argument #1 must be of type array, false given",
            "|array_merge(): Argument #2 must be of type array, false given",
            "|array_diff(): Argument #1 ($array) must be of type array, false given",
            "|array_diff(): Argument #2 must be of type array, false given",
            "|array_intersect_key(): Argument #2 must be of type array, false given",
            "|array_map(): Argument #2 ($array) must be of type array, false given",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the BY-REFERENCE ordering receivers throw rather than fatally refusing.
///
/// `sort($d)` on a `scandir()` failure is the motivating case: the receiver is written
/// back only after the type check, so a non-array leaves through a catchable TypeError.
#[test]
fn execute_program_by_reference_array_mutators_throw_php_type_error() {
    let program = parse_fragment(
        br#"$false = false;
$a = $false;
try { sort($a); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
$b = $false;
try { rsort($b); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
$c = $false;
try { array_push($c, 1); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
$d = $false;
try { array_pop($d); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
$e2 = $false;
try { reset($e2); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
$sorted = [3, 1, 2];
sort($sorted);
echo implode(",", $sorted);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "sort(): Argument #1 ($array) must be of type array, false given",
            "|rsort(): Argument #1 ($array) must be of type array, false given",
            "|array_push(): Argument #1 ($array) must be of type array, false given",
            "|array_pop(): Argument #1 ($array) must be of type array, false given",
            "|reset(): Argument #1 ($array) must be of type array, false given",
            "|1,2,3",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies an eval-DECLARED class names itself, not the runtime cell's backing class.
#[test]
fn execute_program_array_builtin_type_error_names_the_eval_declared_class() {
    let program = parse_fragment(
        br#"class EvalHaystackImposter {}
$imposter = new EvalHaystackImposter();
try { in_array("x", $imposter); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_values($imposter); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "in_array(): Argument #2 ($haystack) must be of type array, EvalHaystackImposter given",
            "|array_values(): Argument #1 ($array) must be of type array, EvalHaystackImposter given",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the single-array projections all reach the shared check.
#[test]
fn execute_program_single_array_builtins_throw_php_type_error() {
    let program = parse_fragment(
        br#"$false = false;
try { array_values($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_unique($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_flip($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_sum($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_product($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_reverse($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_slice($false, 0); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_filter($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_count_values($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_pad($false, 1, 0); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_column($false, "x"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_search("x", $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "array_values(): Argument #1 ($array) must be of type array, false given",
            "|array_unique(): Argument #1 ($array) must be of type array, false given",
            "|array_flip(): Argument #1 ($array) must be of type array, false given",
            "|array_sum(): Argument #1 ($array) must be of type array, false given",
            "|array_product(): Argument #1 ($array) must be of type array, false given",
            "|array_reverse(): Argument #1 ($array) must be of type array, false given",
            "|array_slice(): Argument #1 ($array) must be of type array, false given",
            "|array_filter(): Argument #1 ($array) must be of type array, false given",
            "|array_count_values(): Argument #1 ($array) must be of type array, false given",
            "|array_pad(): Argument #1 ($array) must be of type array, false given",
            "|array_column(): Argument #1 ($array) must be of type array, false given",
            "|array_search(): Argument #2 ($haystack) must be of type array, false given",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the projections whose FIRST argument is `$array` but sat outside the table.
///
/// `array_reduce()` checks argument #1 BEFORE it looks at the callable, measured: a
/// nonexistent callback name still reports the array's type, never a callable error.
#[test]
fn execute_program_array_keys_chunk_reduce_reject_non_array_with_php_type_error() {
    let program = parse_fragment(
        br#"$false = false;
try { array_keys($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_keys(42); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_chunk($false, 2); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_chunk("s", 2); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_reduce($false, "evalReduceNever"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_reduce(new stdClass(), "evalReduceNever"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|" . count(array_keys(["a" => 1, "b" => 2]));
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "array_keys(): Argument #1 ($array) must be of type array, false given",
            "|array_keys(): Argument #1 ($array) must be of type array, int given",
            "|array_chunk(): Argument #1 ($array) must be of type array, false given",
            "|array_chunk(): Argument #1 ($array) must be of type array, string given",
            "|array_reduce(): Argument #1 ($array) must be of type array, false given",
            "|array_reduce(): Argument #1 ($array) must be of type array, stdClass given",
            "|2",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the pair builtins that name their arguments something other than `$array`.
///
/// `array_combine()` validates BOTH slots and calls them `$keys` and `$values`;
/// `array_fill_keys()` names its first `$keys`. Neither spelling is `$array`, so the
/// table has to carry the name rather than assume one.
#[test]
fn execute_program_array_combine_and_fill_keys_name_their_own_parameters() {
    let program = parse_fragment(
        br#"$false = false;
try { array_combine($false, []); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_combine([], $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_combine([], 1.5); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_combine($false, $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_fill_keys($false, 1); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_fill_keys(null, 1); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|" . count(array_combine(["a"], [1]));
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "array_combine(): Argument #1 ($keys) must be of type array, false given",
            "|array_combine(): Argument #2 ($values) must be of type array, false given",
            "|array_combine(): Argument #2 ($values) must be of type array, float given",
            // Both slots are wrong, and php reports the FIRST one.
            "|array_combine(): Argument #1 ($keys) must be of type array, false given",
            "|array_fill_keys(): Argument #1 ($keys) must be of type array, false given",
            "|array_fill_keys(): Argument #1 ($keys) must be of type array, null given",
            "|1",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `array_key_exists()` validates its SECOND argument, which php names `$array`.
#[test]
fn execute_program_array_key_exists_validates_its_second_argument() {
    let program = parse_fragment(
        br#"$false = false;
try { array_key_exists("k", $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_key_exists("k", "str"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_key_exists("k", new stdClass()); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|" . (array_key_exists("k", ["k" => 1]) ? "yes" : "no");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "array_key_exists(): Argument #2 ($array) must be of type array, false given",
            "|array_key_exists(): Argument #2 ($array) must be of type array, string given",
            // php 8 dropped array_key_exists()'s object support, so a class name is reported.
            "|array_key_exists(): Argument #2 ($array) must be of type array, stdClass given",
            "|yes",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the internal-pointer READERS throw where they used to fatally refuse.
///
/// `current()` and `key()` accept `array|object` in php, and an OBJECT is answered
/// rather than rejected — `current(new stdClass())` is `false`, measured — so only a
/// scalar or null reaches the TypeError, whose wording still says plain `array`.
#[test]
fn execute_program_array_pointer_readers_throw_php_type_error() {
    let program = parse_fragment(
        br#"$false = false;
try { current($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { key($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
$int = 42;
try { current($int); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { key("str"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
$list = [7, 8];
echo current($list) . "," . key($list);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "current(): Argument #1 ($array) must be of type array, false given",
            "|key(): Argument #1 ($array) must be of type array, false given",
            "|current(): Argument #1 ($array) must be of type array, int given",
            "|key(): Argument #1 ($array) must be of type array, string given",
            "|7,0",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies the iterator helpers word their expectation `Traversable|array`, not `array`.
///
/// These two are the family's third wording: an ARRAY is accepted, a Traversable object
/// is accepted, and a plain object is rejected BY CLASS NAME against the union type.
#[test]
fn execute_program_iterator_helpers_expect_traversable_or_array() {
    let program = parse_fragment(
        br#"$false = false;
try { iterator_to_array($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { iterator_to_array(new stdClass()); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { iterator_count($false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { iterator_count(new stdClass()); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { iterator_count("str"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|" . iterator_count([1, 2, 3]) . "," . count(iterator_to_array([1, 2]));
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, false given",
            "|iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, stdClass given",
            "|iterator_count(): Argument #1 ($iterator) must be of type Traversable|array, false given",
            "|iterator_count(): Argument #1 ($iterator) must be of type Traversable|array, stdClass given",
            "|iterator_count(): Argument #1 ($iterator) must be of type Traversable|array, string given",
            "|3,2",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `implode()`'s THREE measured refusal stages, in php's own order.
///
/// The `?` in `?array` is php's spelling of the declared type and belongs in the message.
/// It also has a consequence: `null` SATISFIES that declaration, so php cannot refuse it
/// there and words a separate sentence naming both parameters instead — the two null and
/// non-null spellings are different strings, not one message with a swapped type name.
///
/// Argument #1 is checked first, so `implode([], ",")` reports the SEPARATOR; checking the
/// array first would answer the wrong argument number for that call.
///
/// The legacy REVERSED spelling is not a separate signature in php 8:
/// `implode(false, ["a", "b"])` simply coerces `false` to the empty separator and joins,
/// measured — it must NOT throw, which is why the tail asserts a VALUE and not a message.
#[test]
fn execute_program_implode_names_its_nullable_array_parameter() {
    let program = parse_fragment(
        br#"$false = false;
try { implode(",", $false); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { implode(",", "str"); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { implode(",", 42); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { implode(",", new stdClass()); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { implode([], ","); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { implode(",", null); echo "no-throw"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|" . implode($false, ["a", "b"]) . "|" . implode(",", ["a", "b"]);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "implode(): Argument #2 ($array) must be of type ?array, false given",
            "|implode(): Argument #2 ($array) must be of type ?array, string given",
            "|implode(): Argument #2 ($array) must be of type ?array, int given",
            "|implode(): Argument #2 ($array) must be of type ?array, stdClass given",
            // Argument #1 is checked FIRST: this reports the SEPARATOR, not the array.
            "|implode(): Argument #1 ($separator) must be of type string, array given",
            // `?array` lets null past the declaration, so php words the refusal differently.
            "|implode(): If argument #1 ($separator) is of type string, \
             argument #2 ($array) must be of type array, null given",
            "|ab|a,b",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies an out-of-range `count()` mode is php's catchable `ValueError`, not a fatal.
///
/// php names the two accepted constants in the message rather than the offending value,
/// and it raises the ValueError BEFORE the `Countable|array` check: `count(false, 99)`
/// reports the mode, not the value, measured.
#[test]
fn execute_program_count_rejects_an_out_of_range_mode_with_php_value_error() {
    let program = parse_fragment(
        br#"try { count([1], 99); echo "no-throw"; } catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
try { count([1], -1); echo "no-throw"; } catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
try { count(false, 99); echo "no-throw"; } catch (ValueError $e) { echo $e->getMessage(); }
echo "|" . count([1, [2, 3]], COUNT_RECURSIVE) . "," . count([1, [2, 3]], COUNT_NORMAL);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "count(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE",
            "|count(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE",
            "|count(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE",
            "|4,2",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
