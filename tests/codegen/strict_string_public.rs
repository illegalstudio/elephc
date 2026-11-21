//! Purpose:
//! Verifies independently admitted string semantics through public compiler options.
//!
//! Called from:
//! - Focused CLI checks after native and Magician string host parity.
//!
//! Key details:
//! - Explicit formatting remains available while concatenation keeps original kinds.
//! - No string option silently enables the independent coercion policy.

use crate::support::*;

/// Public strings reject original source before either optimizer erases the operation.
#[test]
fn test_strict_strings_public_source_and_independent_coercions() {
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        for source in [
            "<?php echo 1 . \"x\";",
            "<?php declare(strict_types=0); $value = false; echo $value . \"x\";",
            "<?php echo [1];",
            "<?php eval('echo 1 . \"x\";');",
        ] {
            StrictFixture::new(source).assert_error(&["--strict-strings", optimization], "strict-strings");
        }
        let source = r#"<?php
function stringBoundary(string $value): string { return $value . 'x'; }
echo stringBoundary(7), '|', (string) 3 . 'x', '|', str_repeat('a', 2.0);
"#;
        assert_eq!(StrictFixture::new(source).run(&["--strict-strings", optimization]), "7x|3x|aa");
        assert_eq!(StrictFixture::new("<?php echo 7 . 'x';")
            .run(&["--strict-strings", "--no-strict-strings", optimization]), "7x");
    }
}
