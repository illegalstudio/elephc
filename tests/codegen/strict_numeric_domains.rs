//! Purpose:
//! Exercises staged numeric domain failures and ordinary overflow behavior in native programs.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Both optimizers must preserve discarded throwing operations and release fresh operands.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Arithmetic and coercion domains reject PHP scalar conversion without enabling integer overflow.
#[test]
fn test_strict_numeric_domain_native_failures_power_and_optimizer_ownership() {
    let source = r#"<?php
function domainSum(mixed $a, mixed $b): mixed { return $a + $b; }
function domainMod(mixed $a, mixed $b): int { return $a % $b; }
function domainPow(mixed $a, mixed $b): mixed { return $a ** $b; }
function domainBits(mixed $a, mixed $b): int { return $a & $b; }
function domainDiscard(mixed $a): void { try { $a + 1; } catch (TypeError $error) { echo 'T'; } }
function domainOwned(int $a): string { return 'owned-' . $a; }
try { domainSum('2', 1); } catch (TypeError $error) { echo 'D'; }
try { domainSum(true, 1); } catch (TypeError $error) { echo 'B'; }
try { domainSum(null, 1); } catch (TypeError $error) { echo 'N'; }
try { domainMod(7.0, 2); } catch (TypeError $error) { echo 'F'; }
try { domainBits('a', 'b'); } catch (TypeError $error) { echo 'S'; }
try { echo domainPow(2, -1); } catch (ArithmeticError $error) { echo 'P'; }
echo '|', domainPow(2.0, -1), '|', domainSum(2, 3), '|', domainMod(7, 2);
echo is_float(domainSum(PHP_INT_MAX, 1)) ? 'O' : 'I';
domainDiscard(domainOwned($argc));
try { domainSum(domainOwned($argc), domainOwned($argc + 1)); } catch (TypeError $error) { echo 'R'; }
"#;
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let source = if feature == StrictFeature::Arithmetic {
            format!("{source}\necho is_float(4 / 2) ? 'F' : 'I';")
        } else { source.to_string() };
        for optimize in [false, true] {
            let output = StrictFixture::new(&source).capture_staged_lowering(
                SemanticProfile::PERMISSIVE.with(feature, true), optimize);
            assert!(output.success, "{feature:?}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, if feature == StrictFeature::Arithmetic { "DBNFSP|0.5|5|1OTRF" }
                else { "DBNFS0.5|0.5|5|1OTR" }, "{feature:?}, opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{feature:?}, opt={optimize}: {}", output.stderr);
        }
    }
}


/// Direct, spread, prepared and indirect numeric builtins guard original kinds and preserve owners.
#[test]
fn test_strict_numeric_parameters_native_call_surfaces_and_failed_results() {
    let source = r#"<?php
function numericAbs(mixed $value): mixed { return abs($value); }
function numericIntdiv(mixed $left, mixed $right): int { return intdiv($left, $right); }
function numericValue(mixed $value): mixed { return $value; }
echo numericAbs(-3), ':', numericAbs(-2.5), ':', numericIntdiv(9, 2), '|';
echo abs(num: -4), ':', intdiv(...[9, 2]), '|';
$absolute = abs(...); $divide = intdiv(...);
echo $absolute(-5), ':', $divide(11, 2), '|';
echo call_user_func('abs', -6), ':', call_user_func_array('intdiv', [13, 2]), '|';
$result = 23;
try { $result = numericAbs('2'); } catch (TypeError $error) { echo 'S'; }
try { numericAbs(true); } catch (TypeError $error) { echo 'B'; }
try { numericAbs(null); } catch (TypeError $error) { echo 'N'; }
try { numericAbs([1]); } catch (TypeError $error) { echo 'A'; }
try { numericIntdiv(9.0, 2); } catch (TypeError $error) { echo 'F'; }
try { numericIntdiv(9, '2'); } catch (TypeError $error) { echo 'D'; }
try { $absolute(numericValue('2')); } catch (TypeError $error) { echo 'P'; }
try { $divide(numericValue(true), 2); } catch (TypeError $error) { echo 'Q'; }
try { call_user_func('abs', numericValue('2')); } catch (TypeError $error) { echo 'C'; }
try { call_user_func_array('intdiv', [numericValue('9'), 2]); } catch (TypeError $error) { echo 'R'; }
echo ':', $result, '|';
try { numericIntdiv(PHP_INT_MIN, -1); } catch (ArithmeticError $error) { echo 'I'; }
try { $magnitude = numericAbs(PHP_INT_MIN); echo is_float($magnitude) ? 'F' : 'X'; }
catch (ArithmeticError $error) { echo 'O'; }
echo '|', fdiv(1.0, numericAbs(-0.0)) > 0.0 ? 'Z' : 'X';
"#;
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for overflow in [false, true] {
            let expected = format!("3:2.5:4|4:4|5:5|6:6|SBNAFDPQCR:23|I{}|Z", if overflow { "O" } else { "F" });
            for optimize in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, overflow);
                let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
                assert!(output.success, "{feature:?}, overflow={overflow}, opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
                assert_eq!(output.stdout, expected, "{feature:?}, overflow={overflow}, opt={optimize}");
                assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{feature:?}, overflow={overflow}, opt={optimize}: {}", output.stderr);
            }
        }
    }
}
