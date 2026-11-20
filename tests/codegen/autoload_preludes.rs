//! Purpose:
//! Exercises Random, BcMath, GMP, DateTimeZone::listIdentifiers, and preg_grep when
//! only Composer-autoloaded classes use those surfaces.
//!
//! Called from:
//! - The codegen integration harness on every executable supported target.
//!
//! Key details:
//! - Compiles through the production CLI. The entry file never names the surfaces
//!   and the command never passes `--with-random`, `--with-bcmath`, or `--with-gmp`.
//! - GMP is its own test so a missing managed `gmp` package skips only that case.
//!   A cache whose toolchain fingerprint is stale still has to typecheck; the
//!   test then checks that `--emit-asm` emits the GMP bridge call.
//! - Expected numbers match PHP 8.5: `Mt19937(42)->getInt(1, 6)` is 1,
//!   `mt_srand(42); mt_rand(1, 100)` is 43, and `Number("1.5")->add("2.25")` is 3.75.

use crate::support::*;
use std::path::{Path, PathBuf};

/// Owns one Composer fixture until the assertions finish.
struct AutoloadPreludeProject(PathBuf);

impl Drop for AutoloadPreludeProject {
    /// Removes only this test's project on success and unwinding.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Writes the PSR-4 map and one autoloaded class file.
fn write_composer_class(dir: &Path, file: &str, source: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    if !dir.join("composer.json").exists() {
        fs::write(
            dir.join("composer.json"),
            r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
        )
        .unwrap();
    }
    fs::write(dir.join("src").join(file), source).unwrap();
}

/// Compiles `main.php` with the production CLI and no prelude force flag.
fn compile_autoload_project(dir: &Path, bridges: &[&str]) -> std::process::Output {
    ensure_cli_bridge_staticlibs(bridges);
    let mut command = elephc_cli_command(dir);
    configure_prebuilt_cli_bridges(&mut command);
    command
        .args(["--target", target().as_str(), "main.php"])
        .output()
        .expect("run autoload prelude compiler")
}

/// Random, BcMath, and listIdentifiers used only from autoloaded classes compile and run.
#[test]
fn test_autoload_random_bcmath_and_list_identifiers() {
    let project = AutoloadPreludeProject(make_cli_test_dir("elephc_autoload_preludes"));
    let dir = &project.0;
    write_composer_class(
        dir,
        "Dice.php",
        r#"<?php
namespace App;
class Dice {
    public function roll(): int {
        return (new \Random\Randomizer(new \Random\Engine\Mt19937(42)))->getInt(1, 6);
    }
    public function legacy(): int {
        \mt_srand(42);
        return \mt_rand(1, 100);
    }
}
"#,
    );
    write_composer_class(
        dir,
        "Money.php",
        r#"<?php
namespace App;
class Money {
    public function sum(): string {
        return (string) (new \BcMath\Number("1.5"))->add("2.25");
    }
}
"#,
    );
    write_composer_class(
        dir,
        "Zones.php",
        r#"<?php
namespace App;
class Zones {
    public function hasEurope(): int {
        $by_class = \count(\DateTimeZone::listIdentifiers(\DateTimeZone::EUROPE)) > 10 ? 1 : 0;
        $by_function = \count(\timezone_identifiers_list()) > 10 ? 1 : 0;
        return $by_class === 1 && $by_function === 1 ? 1 : 0;
    }
}
"#,
    );
    fs::write(
        dir.join("main.php"),
        r#"<?php
$dice = new App\Dice();
$money = new App\Money();
$zones = new App\Zones();
echo $dice->roll(), "\n";
echo $dice->legacy(), "\n";
echo $money->sum(), "\n";
echo $zones->hasEurope(), "\n";
"#,
    )
    .unwrap();
    let compiled = compile_autoload_project(dir, &["elephc_random", "elephc_bcmath"]);
    assert!(
        compiled.status.success(),
        "autoloaded prelude compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(
        output.status.success(),
        "autoloaded prelude execution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "1\n43\n3.75\n1\n"
    );
}

/// `array_rand` calls alone retain the prelude after direct and imported calls are rewritten.
#[test]
fn test_autoload_array_rand_without_other_random_uses() {
    let project = AutoloadPreludeProject(make_cli_test_dir("elephc_autoload_array_rand"));
    let dir = &project.0;
    write_composer_class(
        dir,
        "Picker.php",
        r#"<?php
namespace App;
use function array_rand as choose;
class Picker {
    public function index(): int { return \array_rand([10]); }
    public function key(): string { return choose(["selected" => 20]); }
}
"#,
    );
    fs::write(
        dir.join("main.php"),
        "<?php $picker = new App\\Picker(); echo $picker->index(), \"\\n\", $picker->key(), \"\\n\";",
    )
    .unwrap();
    let compiled = compile_autoload_project(dir, &["elephc_random"]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0\nselected\n");
}

/// Unqualified Random calls and callables fall back globally while entry-file namespace functions win.
#[test]
fn test_autoload_unqualified_random_prelude_functions() {
    let project = AutoloadPreludeProject(make_cli_test_dir("elephc_autoload_random_fallback"));
    let dir = &project.0;
    write_composer_class(
        dir,
        "Seed.php",
        r#"<?php
namespace App;
class Seed {
    public function roll(): int {
        Mt_SrAnD(42);
        return mt_rand(1, 100);
    }
    public function fraction(): bool {
        $value = lcg_value();
        return $value > 0.0 && $value < 1.0;
    }
    public function maximum(): int { return getrandmax(); }
    public function localMaximum(): int { return mt_getrandmax(); }
    public function callableMaximum(): int {
        $maximum = getrandmax(...);
        return $maximum();
    }
}
"#,
    );
    fs::write(
        dir.join("main.php"),
        r#"<?php
namespace App { function mt_getrandmax(): int { return 7; } }
namespace {
    $seed = new App\Seed();
    echo $seed->roll(), "\n", $seed->fraction() ? 1 : 0, "\n";
    echo $seed->maximum(), "\n", $seed->localMaximum(), "\n";
    echo $seed->callableMaximum(), "\n";
}
"#,
    )
    .unwrap();
    let compiled = compile_autoload_project(dir, &["elephc_random"]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "43\n1\n2147483647\n7\n2147483647\n");
}

/// GMP used only from an autoloaded class compiles and prints the successor.
///
/// A current managed artifact links through the production CLI. A stale toolchain
/// fingerprint cannot supply `libgmp.a`, which is independent of prelude injection:
/// the compiler must already be past type checking, and `--emit-asm` must emit the
/// bridge call.
#[test]
fn test_autoload_gmp_prelude() {
    if skip_without_gmp_native("test_autoload_gmp_prelude") {
        return;
    }
    let project = AutoloadPreludeProject(make_cli_test_dir("elephc_autoload_gmp"));
    let dir = &project.0;
    fs::write(dir.join("elephc.toml"), include_str!("../../examples/gmp/elephc.toml")).unwrap();
    fs::write(dir.join("elephc.lock"), include_str!("../../examples/gmp/elephc.lock")).unwrap();
    write_composer_class(
        dir,
        "Big.php",
        r#"<?php
namespace App;
class Big {
    public function next(): string {
        return \gmp_strval(\gmp_add("123456789012345678901234567890", "1"));
    }
}
"#,
    );
    fs::write(
        dir.join("main.php"),
        r#"<?php
$big = new App\Big();
echo $big->next(), "\n";
"#,
    )
    .unwrap();
    let cache = managed_native_cache_root().expect("gmp managed native cache");
    ensure_cli_bridge_staticlibs(&["elephc_gmp"]);
    let mut command = elephc_cli_command(dir);
    configure_prebuilt_cli_bridges(&mut command);
    let compiled = command
        .args(["--target", target().as_str(), "main.php"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("run autoload gmp compiler");
    if compiled.status.success() {
        let output = run_binary_with_args(&dir.join("main"), dir, &[]);
        assert!(
            output.status.success(),
            "autoloaded GMP execution failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "123456789012345678901234567891\n"
        );
        return;
    }
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    assert!(
        stderr.contains("native artifact is missing or corrupt"),
        "autoloaded GMP compilation failed: {stderr}"
    );
    let mut emitted = elephc_cli_command(dir);
    configure_prebuilt_cli_bridges(&mut emitted);
    let emitted = emitted
        .args(["--target", target().as_str(), "--emit-asm", "main.php"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("emit autoload gmp assembly");
    assert!(
        emitted.status.success(),
        "autoloaded GMP assembly failed: {}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    let user_asm = fs::read_to_string(dir.join("main.s")).expect("read autoload gmp assembly");
    assert!(
        user_asm.contains("elephc_gmp_binary"),
        "autoloaded GMP assembly did not call the bridge"
    );
}

/// `preg_grep()` called only from an autoloaded class still gets its prelude declaration.
#[test]
fn test_autoload_preg_grep() {
    let project = AutoloadPreludeProject(make_cli_test_dir("elephc_autoload_preg_grep"));
    let dir = &project.0;
    write_composer_class(
        dir,
        "Digits.php",
        r#"<?php
namespace App;
class Digits {
    public function keep(array $items): array {
        $kept = preg_grep('/^\d+$/', $items);
        return $kept === false ? [] : $kept;
    }
}
"#,
    );
    fs::write(
        dir.join("main.php"),
        r#"<?php
echo implode(',', (new App\Digits())->keep(['1', 'x', '23'])), "\n";
"#,
    )
    .unwrap();
    let compiled = elephc_cli_command_with_managed_pcre2(dir)
        .args(["--target", target().as_str(), "main.php"])
        .output()
        .expect("run autoload preg_grep compiler");
    assert!(
        compiled.status.success(),
        "autoloaded preg_grep compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = run_binary_with_args(&dir.join("main"), dir, &[]);
    assert!(
        output.status.success(),
        "autoloaded preg_grep execution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1,23\n");
}
