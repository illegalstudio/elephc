//! Purpose:
//! Exercises declared scalar return guards, callable dispatch and exceptional cleanup natively.
//!
//! Called from:
//! - Focused staged codegen regression tests.
//!
//! Key details:
//! - Same-frame catches, finally bodies and rejected temporary results remain observable.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Scalar and union return guards preserve values, catch scopes and owners under both optimizers.
#[test]
fn test_strict_conversion_returns_native_catches_callables_and_owners() {
    let source = r#"<?php
function returnValue(mixed $v): mixed { return $v; }
function returnFloat(mixed $v): float { return $v; }
function returnString(mixed $v): string { return $v; }
function returnBool(mixed $v): bool { return $v; }
function returnMembers(mixed $v): int|false|null { return $v; }
function returnOwned(int $v): mixed { return 'owned-' . $v; }
function returnCaught(mixed $v): float { try { return $v; } catch (TypeError $e) { echo 'R'; return 2.0; } }
function returnFinally(mixed $v): float { try { return $v; } finally { echo 'Z'; } }
function returnExplicit(mixed $v): float { return (float) $v; }
echo is_float(returnFloat(2.0)) ? 'F' : 'X'; echo returnString('S'); echo returnBool(false) ? 'X' : 'b';
echo ':', returnFloat(4.25), ':', fdiv(1.0, returnFloat(-0.0)), ':', substr(returnString(returnOwned($argc)), 0, 6), '|';
$result = 23;
try { $result = returnFloat(1); } catch (TypeError $e) { echo 'N'; }
try { returnString(1); } catch (TypeError $e) { echo 'S'; }
try { returnBool(1); } catch (TypeError $e) { echo 'B'; }
try { returnFloat(returnOwned($argc)); } catch (TypeError $e) { echo 'O'; }
echo returnCaught(1);
try { returnFinally(1); } catch (TypeError $e) { echo 'T'; }
$call = returnFloat(...); try { $call(1); } catch (TypeError $e) { echo 'C'; }
try { call_user_func('returnFloat', 1); } catch (TypeError $e) { echo 'U'; }
$arrow = fn(mixed $v): float => $v; try { $arrow(1); } catch (TypeError $e) { echo 'A'; }
try { array_map('returnFloat', [1]); } catch (TypeError $e) { echo 'M'; }
echo returnMembers(1), returnMembers(false) === false ? 'f' : 'X', returnMembers(null) === null ? 'n' : 'X';
try { returnMembers(true); } catch (TypeError $e) { echo 'Q'; }
echo ':', returnExplicit(1), ':', $result;
"#;
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true), optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "FSb:4.25:-INF:owned-|NSBOR2ZTCUAM1fnQ:1:23", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}
