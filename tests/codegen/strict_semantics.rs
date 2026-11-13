//! Purpose:
//! Exercises independently selected strict guarantees through the real CLI pipeline.
//!
//! Called from:
//! - The focused codegen integration test binary.
//!
//! Key details:
//! - Each fixture uses the existing isolated project and cleanup helpers.

use crate::support::*;

/// Keeps verified literal eval native with either EIR optimization setting.
#[test]
fn test_strict_dynamic_code_literal_eval_optimization_parity() {
    for optimization in ["--ir-opt=on", "--ir-opt=off"] {
        assert_eq!(StrictFixture::new("<?php eval('echo 42;');").run(&["--strict-dynamic-code", optimization]), "42");
    }
}

/// Refuses opaque source before folding and refuses unsupported interpreter fallbacks.
#[test]
fn test_strict_dynamic_code_refuses_runtime_source_and_literal_bridge_fallback() {
    for source in [
        "<?php $code = $argc > 0 ? 'echo 42;' : ''; eval($code);",
        "<?php if (false) { eval($code); }",
        "<?php eval('class StrictLiteralBridgeClass {}');",
    ] {
        StrictFixture::new(source).assert_error(&["--strict-dynamic-code"], "strict-dynamic-code");
    }
}

/// A separate strict feature can install its native eval profile without enabling dynamic-code restrictions.
#[test]
fn test_strict_dynamic_code_override_keeps_independent_locals_eval_context() {
    assert_eq!(StrictFixture::new("<?php echo eval('class StrictProfileContextClass {} return 17;');")
        .run(&["--strict-locals", "--no-strict-dynamic-code"]), "17");
}

/// Included physical source is checked before frontend simplification can discard its branch.
#[test]
fn test_strict_dynamic_code_validates_included_physical_source() {
    let fixture = StrictFixture::new("<?php require __DIR__ . '/included.php';");
    fixture.file("included.php", "<?php if (false) { eval($source); }");
    fixture.assert_error(&["--strict-dynamic-code"], "strict-dynamic-code");
}

/// Composer-loaded declarations retain the same source restrictions as the entry file.
#[test]
fn test_strict_dynamic_code_validates_autoloaded_physical_source() {
    let fixture = StrictFixture::new("<?php $service = new StrictAutoloadService();");
    fixture.file("composer.json", r#"{"autoload":{"classmap":["StrictAutoloadService.php"]}}"#);
    fixture.file("StrictAutoloadService.php",
        "<?php class StrictAutoloadService { public function run(): void { if (false) { eval($source); } } }");
    fixture.assert_error(&["--strict-dynamic-code"], "strict-dynamic-code");
}
