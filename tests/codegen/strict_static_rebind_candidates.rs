//! Purpose:
//! Checks proposed native static declarations do not constrain a source before binding commits.
//!
//! Called from:
//! - Strict Casts native reference publication audits.
//!
//! Key details:
//! - Stringable coercion can re-enter PHP and replace an otherwise unconstrained source.
//! - The accepted string replaces that source before the native declaration becomes live.

use crate::support::*;

/// Proposed reference declarations validate a snapshot without becoming visible during conversion.
#[test]
fn test_strict_static_reference_proposed_source_is_not_live_during_conversion() {
    let source = r#"<?php
class NativeProposedReference { public static string $text = 'old'; }
$fragment = $argc > 0 ? '
class EvalProposedSource {
    public function __toString(): string {
        global $source; echo "S"; $source = ["changed"]; return "new";
    }
}
$source = new EvalProposedSource();
try { NativeProposedReference::$text =& $source; } catch (TypeError $error) { echo "T"; unset($error); }
echo gettype($source), ":", is_string($source) ? $source : "X", ":", NativeProposedReference::$text;
' : '';
eval($fragment);
gc_collect_cycles();
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-casts", optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "Sstring:new:new", "{optimization}: {}",
            String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
