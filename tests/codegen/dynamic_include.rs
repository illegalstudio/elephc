//! Purpose:
//! Integration tests for include/require sites whose path is only known at runtime, closed by an
//! `elephc.includes.toml` manifest (candidates / deny), and for the compile-time folding of
//! `PHP_VERSION` conditions in include paths.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The manifest is found walking up from the site's file, so each fixture ships its own at the
//!   project root; site ids are `<file>@<Class::method>#<ordinal>` and the md5 pins `L.php`.

use crate::support::*;

/// The class holding every dynamic site of these fixtures.
const LOADER: &str = "<?php\nclass L {\n    public function v($p) { return require $p; }\n    public function s($p) { $x = 'L'; include $p; return $x; }\n    public function i($p) { return include $p; }\n    public function o($p) { return include_once $p; }\n    public function d($p) { return strlen('k') . (require $p); }\n    public function n($p) { require $p; }\n}\n";

/// The manifest for [`LOADER`] (its md5 is pinned below).
const MANIFEST: &str = "version = 1\n[[site]]\nid = 'L.php@L::v#1'\nmd5 = '6c9176ee051e698ff972c21e295a5af9'\ncandidates = ['conf/*.php']\n[[site]]\nid = 'L.php@L::s#1'\nmd5 = '6c9176ee051e698ff972c21e295a5af9'\ncandidates = ['conf/*.php']\n[[site]]\nid = 'L.php@L::i#1'\nmd5 = '6c9176ee051e698ff972c21e295a5af9'\ncandidates = ['conf/a.php']\n[[site]]\nid = 'L.php@L::o#1'\nmd5 = '6c9176ee051e698ff972c21e295a5af9'\ncandidates = ['conf/a.php']\n[[site]]\nid = 'L.php@L::d#1'\nmd5 = '6c9176ee051e698ff972c21e295a5af9'\ndeny = \"no d\"\n[[site]]\nid = 'L.php@L::n#1'\nmd5 = '6c9176ee051e698ff972c21e295a5af9'\ndeny = \"no n\"\n";

/// md5 of [`LOADER`], as the manifest pins it.
const LOADER_MD5: &str = "6c9176ee051e698ff972c21e295a5af9";

/// Builds a fixture: the loader, the manifest, two candidates and `main.php`.
fn fixture<'a>(main: &'a str, manifest: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("L.php", LOADER),
        ("elephc.includes.toml", manifest),
        ("conf/a.php", "<?php\nreturn 'A';\n"),
        ("conf/b.php", "<?php\n$x = 'B';\nif (true) { return 'b'; }\n"),
        ("conf/other.txt", "plain\n"),
        ("main.php", main),
    ]
}

/// A candidate is dispatched by its runtime realpath, in value and statement position, and
/// keeps the caller's scope (`$x` written by `conf/b.php` is the method's local).
#[test]
fn test_dynamic_include_dispatches_candidates() {
    let out = compile_and_run_files(
        &fixture(
            "<?php\nrequire __DIR__ . '/L.php';\n$l = new L();\n\
             echo $l->o(__DIR__ . '/conf/a.php'), '|';\n\
             $r = $l->o(__DIR__ . '/conf/a.php'); echo $r === true ? 'true' : $r, '|';\n\
             echo $l->v(__DIR__ . '/conf/a.php'), '|';\n\
             echo $l->v(__DIR__ . '/conf/../conf/b.php'), '|';\n\
             echo $l->s(__DIR__ . '/conf/b.php'), '|';\n\
             echo $l->s(__DIR__ . '/conf/a.php'), \"\\n\";\n",
            MANIFEST,
        ),
        "main.php",
    );
    assert_eq!(out, "A|true|A|b|B|L\n");
}

/// An existing file outside the candidate set throws `\Error` naming the site; a missing file
/// keeps PHP's semantics (`include` is `false`, `require` throws "Failed opening required").
#[test]
fn test_dynamic_include_miss_and_missing_file() {
    let out = compile_and_run_files(
        &fixture(
            "<?php\nrequire __DIR__ . '/L.php';\n$l = new L();\n\
             try { $l->v(__DIR__ . '/conf/other.txt'); } catch (\\Error $e) { echo $e->getMessage(), \"\\n\"; }\n\
             var_dump($l->i(__DIR__ . '/conf/nope.php'));\n\
             try { $l->v(__DIR__ . '/conf/nope.php'); } catch (\\Error $e) { echo str_replace(__DIR__, 'D', $e->getMessage()), \"\\n\"; }\n",
            MANIFEST,
        ),
        "main.php",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].starts_with("elephc: dynamic require of '")
            && lines[0].ends_with("other.txt' at L.php@L::v#1 is not one of the 2 candidate(s) listed in elephc.includes.toml"),
        "{out}"
    );
    assert_eq!(lines[1], "bool(false)");
    assert_eq!(lines[2], "Failed opening required 'D/conf/nope.php' (include_path='.:')");
}

/// A `deny` site throws `\Error` with the manifest's reason, in statement position and in an
/// operand (where it replaces the include before value-include hoisting).
#[test]
fn test_dynamic_include_deny_throws_reason() {
    let out = compile_and_run_files(
        &fixture(
            "<?php\nrequire __DIR__ . '/L.php';\n$l = new L();\n\
             try { $l->n('x'); } catch (\\Error $e) { echo $e->getMessage(), \"\\n\"; }\n\
             try { $l->d('x'); } catch (\\Error $e) { echo $e->getMessage(), \"\\n\"; }\n",
            MANIFEST,
        ),
        "main.php",
    );
    assert_eq!(
        out,
        "elephc: dynamic require at L.php@L::n#1 is denied by elephc.includes.toml: no n\n\
         elephc: dynamic require at L.php@L::d#1 is denied by elephc.includes.toml: no d\n"
    );
}

/// A site with no manifest entry is still a compile error, naming its site id.
#[test]
fn test_dynamic_include_unlisted_site_is_compile_error() {
    let manifest = MANIFEST.replace("id = 'L.php@L::n#1'", "id = 'L.php@L::other#1'");
    let err = compile_files_error_message(&fixture("<?php\nrequire __DIR__ . '/L.php';\n", &manifest), "main.php")
        .expect("an unlisted dynamic site must not compile");
    assert!(err.contains("Runtime-dynamic include/require path expressions are not supported"), "{err}");
    assert!(err.contains("dynamic include site `L.php@L::n#1` has no entry in"), "{err}");
}

/// A stale md5 (the file changed since the manifest was reviewed) is a compile error.
#[test]
fn test_dynamic_include_stale_md5_is_compile_error() {
    let manifest = MANIFEST.replace(LOADER_MD5, "00000000000000000000000000000000");
    let err = compile_files_error_message(&fixture("<?php\nrequire __DIR__ . '/L.php';\n", &manifest), "main.php")
        .expect("a stale md5 must not compile");
    assert!(err.contains("dynamic include site `L.php@L::"), "{err}");
    assert!(err.contains(&format!("has md5 {LOADER_MD5}")), "{err}");
    assert!(err.contains("pins 00000000000000000000000000000000"), "{err}");
}

/// A malformed entry (both `candidates` and `deny`) and a glob matching nothing are refused.
#[test]
fn test_dynamic_include_manifest_validation() {
    let both = MANIFEST.replace("deny = \"no n\"", "deny = \"no n\"\ncandidates = ['conf/a.php']");
    let err = compile_files_error_message(&fixture("<?php\nrequire __DIR__ . '/L.php';\n", &both), "main.php")
        .expect("an entry with candidates and deny must not compile");
    assert!(err.contains("must declare exactly one of"), "{err}");
    let empty_glob = MANIFEST.replace("candidates = ['conf/a.php']", "candidates = ['conf/none/*.php']");
    let err = compile_files_error_message(&fixture("<?php\nrequire __DIR__ . '/L.php';\n", &empty_glob), "main.php")
        .expect("a glob matching no file must not compile");
    assert!(err.contains("matches no file"), "{err}");
}

/// `_once` is shared between a static and a dynamic include of the same canonical file.
#[test]
fn test_dynamic_include_once_shares_static_label() {
    let out = compile_and_run_files(
        &fixture(
            "<?php\nrequire __DIR__ . '/L.php';\n$l = new L();\n\
             $r = include_once __DIR__ . '/conf/a.php'; echo $r === true ? 'true' : $r, '|';\n\
             $r = $l->o(__DIR__ . '/conf/../conf/a.php'); echo $r === true ? 'true' : $r, \"\\n\";\n",
            MANIFEST,
        ),
        "main.php",
    );
    assert_eq!(out, "A|true\n");
}

/// `require PHP_VERSION < 8.2 ? A : B` (CarbonPeriod) picks its branch at compile time with
/// PHP 8 string-vs-float comparison: under the default profile `"8.x.0" < 8.2` is false.
#[test]
fn test_php_version_ternary_include_folds() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire PHP_VERSION < 8.2 ? __DIR__ . '/old.php' : __DIR__ . '/new.php';\n\
                 require version_compare(PHP_VERSION, '8.1.0', '>=') ? __DIR__ . '/new.php' : __DIR__ . '/old.php';\n\
                 require PHP_VERSION_ID >= 80000 && PHP_MAJOR_VERSION === 8 ? __DIR__ . '/new.php' : __DIR__ . '/old.php';\n",
            ),
            ("old.php", "<?php\necho 'old';\n"),
            ("new.php", "<?php\necho 'new';\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "newnewnew");
}

/// `--dynamic-includes=report` lists every non-foldable site at once, with its id and status,
/// instead of failing on the first one.
#[test]
fn test_dynamic_includes_report_lists_every_site() {
    let dir = make_cli_test_dir("elephc_dynamic_include_report");
    std::fs::write(dir.join("L.php"), LOADER).unwrap();
    std::fs::write(
        dir.join("main.php"),
        "<?php\nrequire __DIR__ . '/L.php';\nfunction f($p) { include $p; include $p . '.php'; }\n",
    )
    .unwrap();
    let output = elephc_cli_command(&dir)
        .args(["--quiet", "--dynamic-includes=report", "main.php"])
        .output()
        .expect("failed to run elephc");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("dynamic include sites: 8"), "{stdout}");
    for id in ["L.php@L::v#1", "L.php@L::n#1", "L.php@L::d#1", "main.php@f#1", "main.php@f#2"] {
        assert!(stdout.contains(id), "missing {id} in {stdout}");
    }
    assert!(stdout.contains(&format!("md5 = '{LOADER_MD5}'")), "{stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A one-site loader for the CLI fixtures below.
const SITE: &str = "<?php\nclass S {\n    public function i($p) { return include $p; }\n}\n";

/// Its manifest.
const SITE_MANIFEST: &str = "[[site]]\nid = 'S.php@S::i#1'\nmd5 = '1dcc6bd67e2972105e07b87e27867e8d'\ncandidates = ['c/a.php']\n";

/// Writes the one-site fixture into a fresh CLI directory, compiles it with the CLI and returns
/// the directory (the binary is `main` there).
fn compile_site_fixture(prefix: &str, main: &str, manifest: &str) -> (std::path::PathBuf, std::process::Output) {
    let dir = make_cli_test_dir(prefix);
    std::fs::create_dir_all(dir.join("c")).unwrap();
    std::fs::write(dir.join("S.php"), SITE).unwrap();
    std::fs::write(dir.join("elephc.includes.toml"), manifest).unwrap();
    std::fs::write(dir.join("c/a.php"), "<?php\nreturn 'A';\n").unwrap();
    std::fs::write(dir.join("main.php"), main).unwrap();
    let output = elephc_cli_command(&dir)
        .args(["--quiet", "main.php"])
        .output()
        .expect("failed to run elephc");
    (dir, output)
}

/// `./x` and `../x` resolve against the cwd ONLY (php_resolve_path): run from `/`, they miss
/// with PHP's two `E_WARNING`s and `false`. A bare relative path still falls back to the site's
/// directory, after the cwd.
#[test]
fn test_dynamic_include_dot_relative_paths_use_cwd_only() {
    let main = "<?php\nrequire __DIR__ . '/S.php';\n$s = new S();\n\
                set_error_handler(function ($no, $str) { echo $no, ':', $str, \"\\n\"; return true; });\n\
                var_dump($s->i('./c/a.php'));\nvar_dump($s->i('c/a.php'));\n";
    let (dir, output) = compile_site_fixture("elephc_dyn_dot_relative", main, SITE_MANIFEST);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let run = std::process::Command::new(dir.join("main")).current_dir("/").output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        stdout,
        "2:include(./c/a.php): Failed to open stream: No such file or directory\n\
         2:include(): Failed opening './c/a.php' for inclusion (include_path='.:')\n\
         bool(false)\nstring(1) \"A\"\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// An `elephc.includes.toml` entry that matches no site of the program is a compile error.
#[test]
fn test_dynamic_include_orphan_manifest_entry_is_compile_error() {
    let manifest = format!(
        "{SITE_MANIFEST}[[site]]\nid = 'S.php@S::gone#1'\nmd5 = '1dcc6bd67e2972105e07b87e27867e8d'\ndeny = \"x\"\n"
    );
    let (dir, output) = compile_site_fixture(
        "elephc_dyn_orphan",
        "<?php\nrequire __DIR__ . '/S.php';\n",
        &manifest,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "an orphan entry must not compile");
    assert!(stderr.contains("match no include/require site"), "{stderr}");
    assert!(stderr.contains("S.php@S::gone#1"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A namespace that declares its own `version_compare` or `PHP_VERSION_ID` is not folded to
/// the global (PHP calls the namespaced one): the site stays dynamic, here a compile error.
/// A fully qualified `\\PHP_VERSION_ID` in the same namespace still folds.
#[test]
fn test_version_fold_respects_namespace_declarations() {
    for main in [
        "<?php\nnamespace Foo;\nfunction version_compare($a, $b, $o = null) { return false; }\n\
         require version_compare(PHP_VERSION, '8.0', '>=') ? __DIR__ . '/a.php' : __DIR__ . '/b.php';\n",
        "<?php\nnamespace Foo;\nconst PHP_VERSION_ID = 1;\n\
         require PHP_VERSION_ID >= 80000 ? __DIR__ . '/a.php' : __DIR__ . '/b.php';\n",
        // `use function` / `use const` aliases bind the unqualified name too, also in the
        // global namespace.
        "<?php\nnamespace Foo;\nuse function Other\\vc as version_compare;\n\
         require version_compare(PHP_VERSION, '8.0', '>=') ? __DIR__ . '/a.php' : __DIR__ . '/b.php';\n",
        "<?php\nnamespace Foo;\nuse const Other\\VID as PHP_VERSION_ID;\n\
         require PHP_VERSION_ID >= 80000 ? __DIR__ . '/a.php' : __DIR__ . '/b.php';\n",
        "<?php\nuse function Other\\vc as version_compare;\n\
         require version_compare(PHP_VERSION, '8.0', '>=') ? __DIR__ . '/a.php' : __DIR__ . '/b.php';\n",
        "<?php\nuse const Other\\PHP_VERSION;\n\
         require PHP_VERSION < 8.2 ? __DIR__ . '/a.php' : __DIR__ . '/b.php';\n",
    ] {
        let err = compile_files_error_message(
            &[("main.php", main), ("a.php", "<?php\necho 'a';\n"), ("b.php", "<?php\necho 'b';\n")],
            "main.php",
        )
        .expect("a shadowed version name must not be folded");
        assert!(err.contains("ternary path selection"), "{err}");
    }
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nnamespace Foo;\nconst PHP_VERSION_ID = 1;\n\
                 require \\PHP_VERSION_ID >= 80000 ? __DIR__ . '/a.php' : __DIR__ . '/b.php';\n",
            ),
            ("a.php", "<?php\necho 'a';\n"),
            ("b.php", "<?php\necho 'b';\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "a");
}
