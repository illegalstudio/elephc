//! Purpose:
//! Exercises catchable eval integer division against the generated native runtime.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Opaque eval covers real registry and callable dispatch with balanced heap owners.

use crate::support::*;
use elephc_semantics::SemanticProfile;

/// Eval direct, named and callable integer divisions preserve catch and failed assignment semantics.
#[test]
fn test_strict_intdiv_eval_native_catches_callables_and_failed_destinations() {
    let source = r#"<?php
$code = $argc > 0 ? '
$min = -9223372036854775807 - 1; $saved = 23;
try { $saved = intdiv(7, 0); } catch (DivisionByZeroError $error) { echo "Z"; }
try { $saved = intdiv(num2: -1, num1: $min); } catch (ArithmeticError $error) { echo "O"; }
echo ":", $saved, ":", $min;
$callable = intdiv(...);
try { $callable(7, 0); } catch (DivisionByZeroError $error) { echo ":F"; }
try { call_user_func("intdiv", $min, -1); } catch (ArithmeticError $error) { echo ":C"; }
try { call_user_func_array("intdiv", ["num2" => 0, "num1" => 7]); } catch (DivisionByZeroError $error) { echo ":A"; }
echo ":", $callable(7, 2), ":", intdiv(-7, 2);
' : '';
eval($code);
if ($argc < 0) { throw new ArithmeticError('intdiv metadata'); }
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_eval_runtime(SemanticProfile::PERMISSIVE, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "ZO:23:-9223372036854775808:F:C:A:3:-3", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
