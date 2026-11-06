//! Purpose:
//! Covers REPL project restrictions and source-faithful error diagnostics.
//!
//! Called from:
//! - The `codegen::repl` integration test module.
//!
//! Key details:
//! - Exercises the actual cached host without creating its logical project entry file.

use super::{stdout, Fixture};

/// An allowed project prefix and an unrelated prefix produce opposite OPcache verdicts.
#[test]
fn test_repl_opcache_restrict_api_uses_project_directory() {
    let fixture = Fixture::new();
    let project = fixture.0.canonicalize().unwrap();
    let allowed = format!("opcache.restrict_api={}", project.display());
    let denied = format!("opcache.restrict_api={}/other-project/", project.display());
    let source = "is_array(opcache_get_configuration())\nopcache_reset()\n";
    for (restriction, expected, warns) in [
        (allowed.as_str(), "bool(true)\nbool(true)\n", false),
        (denied.as_str(), "bool(false)\nbool(false)\n", true),
    ] {
        let output = fixture.run(source, &["--ini", "opcache.enable_cli=1", "--ini", restriction]);
        assert_eq!(stdout(&output), expected, "{restriction}");
        let error = String::from_utf8_lossy(&output.stderr);
        if warns {
            assert_eq!(error.matches("API is restricted").count(), 2, "{error}");
        } else {
            assert!(error.is_empty(), "{error}");
        }
        assert!(!project.join(".elephc-repl.php").exists());
    }
}

/// Undefined calls retain spelling through namespace resolution while the session stays usable.
#[test]
fn test_repl_undefined_functions_preserve_source_spelling() {
    let fixture = Fixture::new();
    let output = fixture.run(concat!(
        "$n = 41;\n",
        "StrLenX()\n",
        "namespace Demo; StrLenX();\n",
        "namespace Demo; \\StrLenX();\n",
        "namespace Demo; use function Lib\\MissingCall as Nope; nOpE();\n",
        "function ReplTarget($value) { return $value + 1; }\n",
        "REPLTARGET($n)\n",
        "namespace Demo; echo StRlEn('ok'), \"\\n\";\n",
    ), &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "int(41)\nint(42)\n2\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), concat!(
        "Error: Call to undefined function StrLenX()\n",
        "Error: Call to undefined function Demo\\StrLenX()\n",
        "Error: Call to undefined function StrLenX()\n",
        "Error: Call to undefined function Lib\\MissingCall()\n",
    ));
}

/// First-class callable errors preserve case and leave existing state and valid lookup usable.
#[test]
fn test_repl_undefined_function_callables_preserve_source_spelling() {
    let fixture = Fixture::new();
    let output = fixture.run(concat!(
        "$n = 41;\n",
        "StrLenX(...)\n",
        "namespace Demo; StrLenX(...);\n",
        "namespace Demo; \\StrLenX(...);\n",
        "namespace Demo; Other\\StrLenX(...);\n",
        "namespace Demo; use function Lib\\MissingCall as Nope; nOpE(...);\n",
        "function ReplTarget($value) { return $value + 1; }\n",
        "echo (REPLTARGET(...))($n), \"\\n\";\n",
        "namespace Demo; echo (StRlEn(...))('ok'), \"\\n\";\n",
        "$n\n",
    ), &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "int(41)\n42\n2\nint(41)\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), concat!(
        "Error: Call to undefined function StrLenX()\n",
        "Error: Call to undefined function Demo\\StrLenX()\n",
        "Error: Call to undefined function StrLenX()\n",
        "Error: Call to undefined function Demo\\Other\\StrLenX()\n",
        "Error: Call to undefined function Lib\\MissingCall()\n",
    ));
}
