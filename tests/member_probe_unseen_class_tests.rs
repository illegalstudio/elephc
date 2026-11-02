//! Purpose:
//! Guards `property_exists()` / `method_exists()` on a class nothing else in the program names.
//!
//! Called from:
//! - `cargo test --test member_probe_unseen_class_tests` through Rust's test harness.
//!
//! Key details:
//! - php autoloads the class and answers. The native member tables only know the classes the
//!   compilation saw, and `Foo::class` does not make the autoload pass load `Foo`, so the probe
//!   answered `false`. Symfony's `FrameworkExtension` asks
//!   `property_exists(ValidatorExtension::class, 'violationMapper')`, took the "missing" branch
//!   and removed `form.violation_mapper` (and its interface alias) from the container.
//! - The fix is in the autoload walker: a literal / `X::class` first argument of these two probes
//!   pulls the class into the compiled program (no interpreter is linked for it). The shape here
//!   (composer.json PSR-4 map, class named by nothing but the probes) answered four `false` before.
//! - Expected output captured from php 8.5: `true`, `true`, `false`, `false`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), id));
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut exe = std::env::current_exe().expect("failed to resolve current test binary");
        exe.pop();
        if exe.ends_with("deps") {
            exe.pop();
        }
        exe.join("elephc").to_string_lossy().into_owned()
    })
}

const MANIFEST: &str = r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#;

const HIDDEN: &str = r#"<?php
namespace App;

class Hidden
{
    private readonly ?string $violationMapper;
    public function __construct(private int $validator, ?string $violationMapper = null)
    {
        $this->violationMapper = $violationMapper;
    }
    public function mapped(): bool { return true; }
}
"#;

const ENTRY: &str = r#"<?php
use App\Hidden;

function check(): void {
    var_dump(property_exists(Hidden::class, 'violationMapper'));
    var_dump(method_exists('App\Hidden', 'mapped'));
    var_dump(property_exists(Hidden::class, 'nope'));
    var_dump(method_exists(Hidden::class, 'missing'));
}
check();
"#;

/// Probing a class that only the autoloader can supply answers like php instead of `false`.
#[test]
fn property_and_method_exists_autoload_a_class_the_program_never_names() {
    let dir = make_test_dir("member_probe_unseen");
    fs::write(dir.join("composer.json"), MANIFEST).unwrap();
    fs::write(dir.join("src/Hidden.php"), HIDDEN).unwrap();
    fs::write(dir.join("entry.php"), ENTRY).unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("entry.php")
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc entry.php failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(dir.join("entry")).output().expect("failed to run compiled binary");
    assert!(run.status.success(), "binary failed: {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\n"
    );
}
