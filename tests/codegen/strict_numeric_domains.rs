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
