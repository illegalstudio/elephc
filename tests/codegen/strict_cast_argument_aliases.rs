//! Purpose:
//! Audits checked casts of reference arguments selected before later argument effects.
//!
//! Called from:
//! - Casts admission checks across native and eval-owned function/property boundaries.
//!
//! Key details:
//! - Mutable writes update the captured cell; unset/rebind leaves the old cell independent.
//! - Explicit mixed properties isolate alias identity from implicit conversion policy.

use crate::support::*;

/// Coercion snapshots survive source retirement and retain a failed precision handler as previous.
#[test]
fn test_strict_casts_weak_property_sources_callbacks_and_retirement() {
    let source = include_str!("../fixtures/strict/weak_property_reference_sources.php");
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-casts", optimization, "--heap-debug"]);
        assert!(output.status.success(), "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout),
            "SS:new:new;PTypeError:RuntimeException:1:1|42:42:7.9D:replaced",
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Native and eval property declarations inspect the original RHS before weak scalar binding.
#[test]
fn test_strict_casts_native_eval_property_candidates_do_not_chain_coercions() {
    let source = r#"<?php
class NativeCastCandidate { public static float $number = 1.5; }
$fragment = $argc > 0 ? '
class EvalCastCandidate { public int|float $numeric = 1.5; public mixed $open = null; }
$object = new EvalCastCandidate(); $alias = 1.5;
NativeCastCandidate::$number =& $alias;
$alias = 7; echo gettype($alias), ":", $alias, ";";
$object->open =& $alias;
try { $alias = 8; } catch (TypeError $mixedError) { echo "M"; unset($mixedError); }
unset($object->open);
$object->numeric =& $alias;
try { $alias = 8; } catch (TypeError $error) { echo "T"; unset($error); }
echo gettype($alias), ":", $alias, ":", NativeCastCandidate::$number, ";";
unset($object->numeric); $alias = 9; echo gettype($alias), ":", $alias;
' : '';
eval($fragment); gc_collect_cycles();
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-casts", optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "double:7;MTdouble:7:7;double:9",
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Checked explicit mutation observes current cells while preserving detached property selections.
#[test]
fn test_strict_casts_reference_argument_selection_before_later_effects_bundle() {
    let source = r#"<?php
function nativeCastArgument(mixed &$value, int $ignored): void {
    echo 'N'; settype($value, 'int'); echo gettype($value), ':', $value;
}
$fragment = $argc > 0 ? '
class EvalCastArgument { public mixed $value = "42.9"; public float $amount = 1.5; public static mixed $shared = "43.9"; }
function changeCastArgument(EvalCastArgument $object): int { $object->value = "7.9"; return 0; }
function detachCastArgument(EvalCastArgument $object): int {
    unset($object->value); $object->value = "7.9"; return 0;
}
function changeSharedCastArgument(): int { EvalCastArgument::$shared = "9.9"; return 0; }
function evalCastArgument(mixed &$value, int $ignored): void {
    echo "E"; settype($value, "int"); echo gettype($value), ":", $value;
}
$first = new EvalCastArgument(); nativeCastArgument($first->value, changeCastArgument($first));
echo "|", gettype($first->value), ":", $first->value, ";";
$second = new EvalCastArgument(); nativeCastArgument($second->value, detachCastArgument($second));
echo "|", gettype($second->value), ":", $second->value, ";";
$third = new EvalCastArgument(); evalCastArgument($third->value, detachCastArgument($third));
echo "|", gettype($third->value), ":", $third->value, ";";
nativeCastArgument(EvalCastArgument::$shared, changeSharedCastArgument());
echo "|", gettype(EvalCastArgument::$shared), ":", EvalCastArgument::$shared, ";";
nativeCastArgument($first->amount, 0);
echo "|", gettype($first->amount), ":", $first->amount, ";";
evalCastArgument($first->amount, 0);
echo "|", gettype($first->amount), ":", $first->amount, ";";
' : '';
eval($fragment);
gc_collect_cycles();
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-casts", optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), concat!(
            "Ninteger:7|integer:7;Ninteger:42|string:7.9;",
            "Einteger:42|string:7.9;Ninteger:9|integer:9;",
            "Ndouble:1|double:1;Edouble:1|double:1;"), "{optimization}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
