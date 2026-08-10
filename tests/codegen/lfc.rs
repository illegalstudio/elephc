//! Purpose:
//! End-to-end CLI regressions for tagless `.lfc` source and mixed PHP/LFC projects.
//!
//! Called from:
//! - `cargo test --test codegen_tests lfc` through the integration test harness.
//!
//! Key details:
//! - The real CLI selects each physical file's mode and derives normal output paths from its stem.
//! - Dynamic-eval coverage uses CLI assembly output plus the test-only native linker provider.

use crate::support::*;

/// Writes a temporary project, compiles its entry through the CLI, and returns native stdout.
fn compile_lfc_project_and_run(
    files: &[(&str, &str)],
    entry: &str,
    flags: &[&str],
) -> String {
    let dir = make_cli_test_dir("elephc_cli_lfc");
    for (path, source) in files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("project directory should be created");
        }
        fs::write(path, source).expect("project source should be written");
    }

    let entry_path = dir.join(entry);
    let compile = elephc_cli_command(&dir)
        .args(flags)
        .arg(&entry_path)
        .output()
        .expect("elephc CLI should run");
    assert!(
        compile.status.success(),
        "LFC compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let binary = entry_path.with_extension("");
    let output = run_binary(&binary, &dir);
    assert!(
        output.status.success(),
        "LFC binary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("program output should be UTF-8");
    let _ = fs::remove_dir_all(dir);
    stdout
}

/// Compiles a mixed-source project through the CLI and links its dynamic-eval assembly with test providers.
fn compile_lfc_eval_project_and_run(
    files: &[(&str, &str)],
    entry: &str,
    flags: &[&str],
) -> String {
    let dir = make_cli_test_dir("elephc_cli_lfc_eval");
    for (path, source) in files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("project directory should be created");
        }
        fs::write(path, source).expect("project source should be written");
    }

    let entry_path = dir.join(entry);
    let compile = elephc_cli_command(&dir)
        .args(flags)
        .arg("--emit-asm")
        .arg(&entry_path)
        .output()
        .expect("elephc CLI should run");
    assert!(
        compile.status.success(),
        "LFC eval assembly compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let user_asm = fs::read_to_string(entry_path.with_extension("s"))
        .expect("CLI should emit LFC eval assembly");
    let runtime_features = elephc::codegen::RuntimeFeatures {
        regex: true,
        mb_strlen: false,
        phar_archive: false,
        descriptor_invoker: true,
        eval_bridge: true,
        eval_scope: true,
        web: false,
        pdo_udf: false,
        // The LFC fixture is an eval program with no Fiber and no Generator, so the
        // stack-releasing arms of `__rt_object_free_deep` are not emitted for it.
        fiber: false,
        generator: false,
        // It opens no directory and no pipe either, so `__rt_mixed_free_deep` carries
        // neither kind-specific destructor arm.
        popen_resource: false,
        directory_resource: false,
    };
    let runtime_asm =
        elephc::codegen::generate_runtime_with_features(8_388_608, target(), runtime_features);
    let mut checker_libraries = vec!["elephc_tz".to_string()];
    if target().platform == Platform::MacOS {
        checker_libraries.push("iconv".to_string());
    }
    let requirements = TestLinkRequirements::new(
        checker_libraries,
        elephc::codegen::link_requirements_for_runtime_features(runtime_features),
    );
    let stdout = assemble_and_run(
        &user_asm,
        &runtime_obj_for_asm(&runtime_asm),
        &dir,
        &requirements,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(dir);
    stdout
}

/// Writes a temporary project and returns the diagnostic from an expected CLI failure.
fn compile_lfc_project_error(
    files: &[(&str, &str)],
    entry: &str,
    flags: &[&str],
) -> String {
    let dir = make_cli_test_dir("elephc_cli_lfc_error");
    for (path, source) in files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("project directory should be created");
        }
        fs::write(path, source).expect("project source should be written");
    }
    let compile = elephc_cli_command(&dir)
        .args(flags)
        .arg(dir.join(entry))
        .output()
        .expect("elephc CLI should run");
    assert!(!compile.status.success(), "compilation should fail");
    let stderr = String::from_utf8_lossy(&compile.stderr).into_owned();
    let _ = fs::remove_dir_all(dir);
    stderr
}

/// Verifies a tagless LFC entry compiles to and runs from the normal stem-derived path.
#[test]
fn lfc_entry_compiles_and_runs_without_php_tags() {
    let output = compile_lfc_project_and_run(
        &[("main.lfc", "echo \"tagless\";")],
        "main.lfc",
        &[],
    );
    assert_eq!(output, "tagless");
}

/// Verifies check-only mode accepts LFC input without creating a native artifact.
#[test]
fn lfc_check_mode_accepts_tagless_source() {
    let dir = make_cli_test_dir("elephc_cli_lfc_check");
    let entry = dir.join("main.lfc");
    fs::write(&entry, "echo \"checked\";").expect("LFC source should be written");
    let compile = elephc_cli_command(&dir)
        .args(["--check"])
        .arg(&entry)
        .output()
        .expect("elephc CLI should run");
    assert!(
        compile.status.success(),
        "LFC check failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(!entry.with_extension("").exists());
    let _ = fs::remove_dir_all(dir);
}

/// Verifies strict PHP and LFC retain distinct extension visibility in one include graph.
#[test]
fn lfc_mixed_strict_project_keeps_per_file_extensions_and_defines() {
    ensure_cli_bridge_staticlibs(&["elephc_crypto"]);
    let output = compile_lfc_project_and_run(
        &[
            (
                "main.php",
                r#"<?php
function ptr_is_null(int $value): int { return 7; }
echo ptr_is_null(0), ":";
$strictProbe = "ptr_null";
echo function_exists($strictProbe) ? "P" : "p";
echo ":", is_callable($strictProbe) ? "I" : "i";
$strictName = "ptr_is_null";
echo ":", $strictName(0);
echo ":", call_user_func($strictName, 0);
$strictCallable = ptr_is_null(...);
echo ":", $strictCallable(0);
require "part.lfc";
"#,
            ),
            (
                "part.lfc",
                r#"ifdef FEATURE {
    $lfcProbe = "ptr_null";
    echo ":", function_exists($lfcProbe) ? "L" : "l", ":";
    echo is_callable($lfcProbe) ? "I" : "i", ":";
    $lfcName = "ptr_is_null";
    echo $lfcName(ptr_null()) ? "1" : "0", ":";
    echo call_user_func($lfcName, ptr_null()) ? "1" : "0", ":";
    $lfcCallable = ptr_is_null(...);
    echo $lfcCallable(ptr_null()) ? "1" : "0";
}"#,
            ),
        ],
        "main.php",
        &["--strict-php", "--define", "FEATURE"],
    );
    assert_eq!(output, "7:p:i:7:7:7:L:I:1:1:1");
}

/// Verifies Composer files and PSR-4 discovery both load tagless LFC source in strict builds.
#[test]
fn lfc_composer_autoload_files_and_psr4_use_physical_source_modes() {
    let output = compile_lfc_project_and_run(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"},"files":["src/bootstrap.lfc"]}}"#,
            ),
            (
                "src/bootstrap.lfc",
                r#"ifdef FEATURE {
    echo "B";
}"#,
            ),
            (
                "src/Greeter.lfc",
                r#"namespace App;
ifdef FEATURE {
    echo "C";
}
class Greeter {
    public function marker(): string {
        return "L";
    }
}"#,
            ),
            (
                "main.php",
                "<?php\n$greeter = new App\\Greeter();\necho $greeter->marker();",
            ),
        ],
        "main.php",
        &["--strict-php", "--define", "FEATURE"],
    );
    assert_eq!(output, "BCL");
}

/// Verifies nested once-includes, magic paths, and the OPcache manifest retain LFC paths.
#[test]
fn lfc_nested_includes_preserve_paths_once_guards_and_manifest_entries() {
    let output = compile_lfc_project_and_run(
        &[
            (
                "main.lfc",
                r#"echo basename(__FILE__), ":";
require_once "part.lfc";
require_once "part.lfc";"#,
            ),
            (
                "part.lfc",
                r#"echo basename(__FILE__), ":";
echo opcache_is_script_cached(__FILE__) ? "M" : "m";
require "leaf.php";"#,
            ),
            ("leaf.php", "<?php echo \":P\";"),
        ],
        "main.lfc",
        &["--ini", "opcache.enable_cli=1"],
    );
    assert_eq!(output, "main.lfc:part.lfc:M:P");
}

/// Verifies runtime eval dispatch restores the caller's PHP/LFC builtin profile each time.
#[test]
fn lfc_and_strict_php_eval_calls_do_not_leak_profiles() {
    let output = compile_lfc_eval_project_and_run(
        &[
            (
                "main.php",
                r#"<?php
function strict_eval_profile(string $code): string { return eval($code); }
function strict_literal_eval_profile(): string {
    return eval('return function_exists("ptr_null") ? "L" : "s";');
}
require "part.lfc";
$probe = 'return function_exists("ptr_null") ? "L" : "s";';
echo strict_eval_profile($probe), lfc_eval_profile($probe), strict_eval_profile($probe);
echo ":", strict_literal_eval_profile(), lfc_literal_eval_profile(), strict_literal_eval_profile();
"#,
            ),
            (
                "part.lfc",
                r#"function lfc_eval_profile(string $code): string { return eval($code); }
function lfc_literal_eval_profile(): string {
    return eval('return function_exists("ptr_null") ? "L" : "s";');
}"#,
            ),
        ],
        "main.php",
        &["--strict-php"],
    );
    assert_eq!(output, "sLs:sLs");
}

/// Verifies strict auditing sees PHP `ifdef` before a selected branch can be removed.
#[test]
fn lfc_strict_define_does_not_hide_ifdef_in_php_source() {
    let stderr = compile_lfc_project_error(
        &[
            ("main.lfc", "require \"part.php\";"),
            (
                "part.php",
                "<?php ifdef FEATURE { echo \"selected\"; } else { echo \"other\"; }",
            ),
        ],
        "main.lfc",
        &["--strict-php", "--define", "FEATURE"],
    );
    assert!(
        stderr.contains("ifdef") && stderr.contains("strict-php"),
        "unexpected diagnostic: {stderr}"
    );
}

/// Verifies strict mode still audits a PHP file included from an LFC entry.
#[test]
fn lfc_entry_does_not_disable_strict_php_for_included_php() {
    let stderr = compile_lfc_project_error(
        &[
            ("main.lfc", "require \"part.php\";"),
            ("part.php", "<?php ptr_null();"),
        ],
        "main.lfc",
        &["--strict-php"],
    );
    assert!(
        stderr.contains("Undefined function: ptr_null")
            || stderr.contains("disabled by --strict-php"),
        "unexpected diagnostic: {stderr}"
    );
}

/// Verifies physical PHP tags in an LFC file receive the dedicated lexical diagnostic.
#[test]
fn lfc_rejects_physical_php_tags() {
    let stderr = compile_lfc_project_error(
        &[("main.lfc", "<?php echo 1;")],
        "main.lfc",
        &[],
    );
    assert!(
        stderr.contains("not valid in .lfc"),
        "unexpected diagnostic: {stderr}"
    );
}
