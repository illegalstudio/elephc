//! Purpose:
//! Verifies logical null string reads remain visible at exact value boundaries.
//!
//! Called from:
//! - Focused native Coercions admission regressions.
//!
//! Key details:
//! - Concrete string storage can carry a missing-entry marker without a Mixed box.
//! - Explicit casts and nullable declarations retain their independent contracts.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Exact string arguments, returns and builtin inputs reject missing logical values.
#[test]
fn test_strict_coercions_missing_string_value_boundaries() {
    let source = r#"<?php
function requireOriginalString(string $value): void { echo 'V'; }
function returnOriginalString(int $index): string { $parts = ['a']; return $parts[$index]; }
function acceptNullableString(?string $value): void { echo is_null($value) ? 'N' : 'S'; }
$parts = ['a']; $index = $argc + 3;
try { requireOriginalString($parts[$index]); } catch (TypeError $error) { echo 'T'; unset($error); }
echo '|';
try { echo gettype(returnOriginalString($index)); } catch (TypeError $error) { echo 'T'; unset($error); }
echo '|';
try { echo strlen($parts[$index]); } catch (TypeError $error) { echo 'T'; unset($error); }
echo '|'; acceptNullableString($parts[$index]);
requireOriginalString((string) $parts[$index]);
echo ':', strlen(''), ':', strlen('a');
gc_collect_cycles();
"#;
    for ast in [false, true] {
        for ir in [false, true] {
            let output = StrictFixture::new(source).capture_staged_optimizers(
                SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true), ast, ir);
            assert!(output.success, "ast={ast} ir={ir}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, "T|T|T|NV:0:1", "ast={ast} ir={ir}: {}", output.stderr);
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        }
    }
}
