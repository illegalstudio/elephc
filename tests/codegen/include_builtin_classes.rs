//! Purpose:
//! End-to-end value tests for CONSTRUCTING builtin SPL and date/time classes inside a file that
//! is included at RUN TIME. Symfony's vendor code arrives through the Composer autoloader, which
//! is a non-literal `include`, and 66 of the 95 builtin-class instantiations in that vendor tree
//! are SPL — so this is the shape that decides whether an autoloaded class can allocate at all.
//!
//! Called from:
//! - `cargo test --test codegen_tests include_builtin_classes` through Rust's test harness.
//!
//! Key details:
//! - The include path in `main.php` is DELIBERATELY not a string literal, for the reason
//!   `include_reflection` states: a literal path is resolved and inlined at compile time, which
//!   puts the included declarations into the program the checker walks and makes the fixture
//!   prove nothing. A computed path is the real shape.
//! - For the same reason `main.php` must never name an SPL or date/time class, call `eval`, use
//!   `new $c`, or call `unserialize`. Each of those independently opens
//!   `builtin_spl_classes::program_may_reference_spl` or
//!   `builtin_types::datetime::gate::program_may_reference_datetime`, and the fixture would then
//!   pass without exercising the runtime-include route.
//! - Construction needs THREE halves and each fails at a different depth, so a fixture that only
//!   pins the final value is what catches a half going missing: the checker gate (absent, the
//!   class is not in `class_infos` at all), the forced EIR lowering of `__construct` (absent, the
//!   eval constructor bridge has no dispatch slot), and the `_classes_by_name` row (absent,
//!   `__rt_new_by_name` answers null). Before this change all three were missing for SPL and the
//!   binary printed `Fatal error: eval() runtime failed: could not construct class "ArrayObject"`.
//! - Every expected string is `php -n` 8.5.6's output on the same two files.

use crate::support::*;

/// Verifies a runtime-included file can construct the SPL container classes it names.
///
/// `php -n` 8.5.6 prints `3;a;done`.
#[test]
fn test_runtime_include_constructs_spl_containers() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n",
            ),
            (
                "piece.php",
                "<?php\n$o = new ArrayObject([1, 2, 3]);\necho count($o);\necho \";\";\n$f = new SplFixedArray(2);\n$f[0] = \"a\";\necho $f[0];\necho \";done\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "3;a;done");
}

/// Verifies a runtime-included file can construct the date/time classes it names.
///
/// `php -n` 8.5.6 prints `2020-01-02;2020-01-03;done`. The date/time family already had its
/// constructors force-lowered for eval fragments
/// (`ir_lower::builtin_datetime::lower_eval_date_alias_methods_if_needed`); what it lacked was the
/// checker gate, so the classes were never registered for a runtime include to name.
#[test]
fn test_runtime_include_constructs_date_classes() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n",
            ),
            (
                "piece.php",
                "<?php\n$d = new DateTimeImmutable(\"2020-01-02 03:04:05\");\necho $d->format(\"Y-m-d\");\necho \";\";\n$i = new DateInterval(\"P1D\");\necho $d->add($i)->format(\"Y-m-d\");\necho \";done\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "2020-01-02;2020-01-03;done");
}

/// Verifies a constructed SPL object can be ITERATED, not merely allocated.
///
/// `ArrayIterator` is the fourth most instantiated builtin class in Symfony's vendor tree, and a
/// `foreach` over it is the shape that actually uses the vtable: allocation and `__construct`
/// alone would leave `current`/`key`/`next`/`rewind`/`valid` unemitted and the runtime class-id
/// dispatch would jump through a null slot. `push_builtin_spl_metadata_methods` is what forces
/// them alongside the constructor, and this is the test that would catch its removal.
///
/// `php -n` 8.5.6 prints `56;done`.
#[test]
fn test_runtime_include_iterates_a_constructed_spl_iterator() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n",
            ),
            (
                "piece.php",
                "<?php\n$a = new ArrayIterator([5, 6]);\nforeach ($a as $v) {\n    echo $v;\n}\necho \";done\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "56;done");
}

/// Pins the BOUNDARY this echelon did not move, so nobody mistakes it for a fix that regressed.
///
/// The forced surface is exactly `is_dynamic_new_mixed_metadata_candidate` filtered by
/// `is_supported_builtin_spl_method`, which is the same pair the `new $c` arm uses. Classes with
/// no arm in `is_supported_builtin_spl_method` — `SplObjectStorage` is the notable one, and it is
/// the single most instantiated builtin class in Symfony's vendor tree at 16 sites — still cannot
/// be constructed from a runtime include, because the on-demand SPL lowering has no body it is
/// allowed to produce for them.
///
/// The failure is LOUD and specific: `ELEPHC_EVAL_TRACE=1` reports
/// `phase=native_constructor stage=signature class="SplObjectStorage" details=Some((0, 0, 0, true))`
/// — the signature resolves and is bridge-supported — then
/// `phase=native_constructor_error stage=construct`, because
/// `__elephc_eval_value_construct_object` was handed a resolved target name and found no slot.
///
/// What this test asserts is the half that DOES hold, so the boundary is documented by something
/// executable rather than by a comment: the same class constructs and dispatches perfectly well in
/// AOT, so the gap is the runtime-include route alone, not the class.
#[test]
fn test_spl_object_storage_still_constructs_in_aot_where_the_include_route_cannot() {
    let out = compile_and_run(
        r#"<?php
$o = new SplObjectStorage();
$k = new stdClass();
$o->offsetSet($k, 1);
echo count($o);
echo ";done";
"#,
    );
    assert_eq!(out, "1;done");
}
