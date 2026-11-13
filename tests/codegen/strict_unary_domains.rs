//! Purpose:
//! Exercises strict unary and update domains with native storage and exceptional cleanup.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Failed postfix results, references, properties and shared arrays retain balanced owners.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Unary and update guards survive optimization and leave existing destinations unchanged.
#[test]
fn test_strict_unary_domain_native_failures_storage_and_postfix_owners() {
    let source = r#"<?php
function guardedNegate(mixed $value): mixed { return -$value; }
function guardedComplement(mixed $value): mixed { return ~$value; }
function guardedPost(mixed &$value): mixed { return $value++; }
function guardedPre(mixed &$value): mixed { return --$value; }
function unaryText(int $number): string { return 'abc' . $number; }
function discardUnary(mixed $value): void {
    try { $discardedNegation = -$value; } catch (TypeError $error) { echo 'D'; }
    try { $discardedComplement = ~$value; } catch (TypeError $error) { echo 'G'; }
}
class UnaryDomainCell { public mixed $value; }
try { guardedNegate(true); } catch (TypeError $error) { echo 'T'; }
try { guardedNegate(null); } catch (TypeError $error) { echo 'N'; }
try { guardedNegate(unaryText($argc)); } catch (TypeError $error) { echo 'S'; }
try { guardedComplement(7.5); } catch (TypeError $error) { echo 'F'; }
try { echo bin2hex(guardedComplement('A')); } catch (TypeError $error) { echo 'B'; }
echo '|', guardedNegate(2.5), ':', guardedComplement(7), '|';
$text = unaryText($argc);
try { guardedPost($text); } catch (TypeError $error) { echo 'R'; }
echo ':', $text, '|';
$cell = new UnaryDomainCell(); $cell->value = unaryText($argc);
try { $cell->value++; } catch (TypeError $error) { echo 'P'; }
$array = [unaryText($argc)]; $copy = $array;
try { $array[0]++; } catch (TypeError $error) { echo 'A'; }
echo ':', $cell->value, ':', $array[0], ':', $copy[0], '|';
discardUnary(unaryText($argc));
$value = 7; echo ':', guardedPost($value), ':', $value;
$value = 7.5; $updated = guardedPre($value); echo is_float($updated) ? ':f' : ':bad', $updated;
$limit = PHP_INT_MAX;
try { $old = guardedPost($limit); echo is_int($old) && is_float($limit) ? ':O' : ':bad'; }
catch (ArithmeticError $error) { echo $limit === PHP_INT_MAX ? ':X' : ':bad'; }
"#;
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for checked in [false, true] {
            for optimize in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, checked);
                let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
                assert!(output.success, "{profile:?}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
                let complement = if feature == StrictFeature::Arithmetic { "B" } else { "be" };
                let discard = if feature == StrictFeature::Arithmetic { "DG" } else { "D" };
                let boundary = if checked { "X" } else { "O" };
                assert_eq!(output.stdout, format!("TNSF{complement}|-2.5:-8|R:abc1|PA:abc1:abc1:abc1|{discard}:7:8:f6.5:{boundary}"),
                    "{profile:?}, opt={optimize}");
                assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{profile:?}, opt={optimize}: {}", output.stderr);
            }
        }
    }
}

/// Typed and dynamic string complements retain PHP byte semantics in the default profile.
#[test]
fn test_strict_unary_domain_default_typed_string_complement_control() {
    let output = compile_and_run_with_heap_debug("<?php function complementText(string $value): mixed { return ~$value; } function complementMixed(mixed $value): mixed { return ~$value; } echo bin2hex(complementText('A')), ':', bin2hex(complementMixed('A')); ");
    assert!(output.success, "{} {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "be:be");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
