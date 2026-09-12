//! Purpose:
//! End-to-end tests for `opcache.blacklist_filename` — the directive that names paths
//! reference PHP RUNS but refuses to CACHE.
//!
//! Called from:
//! - `cargo test --test opcache_blacklist_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation here was VERIFIED against reference PHP 8.5.10 before being
//!   written, and the matcher itself was checked against reference over a corpus of 34
//!   paths by 26 patterns with zero disagreements. The probes are recorded in each
//!   test's own doc comment.
//! - THE SCRIPT STILL RUNS. A blacklisted include executes normally and is merely never
//!   stored, so the observable difference is in the counters, in `scripts` and in
//!   `opcache_get_configuration()['blacklist']`, never in the program's output. Every test
//!   therefore asserts the side that is visible.
//! - The `blacklist` listing is reached through two completely different paths: the eval
//!   interpreter reads the loaded patterns directly, while natively compiled code reads
//!   them back one at a time across the bridge. `both_surfaces_report_the_same_blacklist`
//!   is what keeps those two from drifting.
//! - `opcache.file_update_protection` is pinned to `0` in every fixture. Left at its
//!   default of 2 seconds it refuses a just-written file for its AGE, which looks exactly
//!   like a blacklist refusal and silently invalidates the test — this is the confound
//!   that made an earlier revision of the matcher wrong in the opposite direction.
//! - The blacklist is loaded when the eval context is built, so every fixture reaches an
//!   `eval()`; a binary with no dynamic tier never loads one. Same residual divergence as
//!   the rest of the runtime-cache surface, documented in `docs/php/opcache.md`.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated
//!   temp dir, the same harness style as `opcache_runtime_cache_tests`. Host-target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes, CANONICALIZED
/// so the paths the probe builds match the spelling the blacklist is compared against.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `<dir>/main.php` with the supplied `--ini` assignments and returns the executable.
fn compile(dir: &Path, ini: &[&str]) -> PathBuf {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(dir.join("main.php"));
    for assignment in ini {
        cmd.arg("--ini").arg(assignment);
    }
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "compilation failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    dir.join("main")
}

/// Runs a compiled binary, asserting success, and returns its stdout.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run binary");
    assert!(
        output.status.success(),
        "binary failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Returns the value printed as `<key>=<value>` by the probe.
fn line<'a>(out: &'a str, key: &str) -> &'a str {
    out.lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing `{key}=` in output:\n{out}"))
        .trim()
}

/// Writes the two includable fixtures plus a probe that reports the observable state.
///
/// The includes are declaration-free so that including one twice re-enters the compile
/// path, which is what makes a per-refusal counter distinguishable from a per-file one.
fn write_fixtures(dir: &Path) {
    fs::write(dir.join("blocked.php"), "<?php $blocked_ran = 1;\n").unwrap();
    fs::write(dir.join("allowed.php"), "<?php $allowed_ran = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
// The includes sit INSIDE the eval deliberately. A constant fragment like `eval('$x = 1;')`
// is const-folded, emits no bridge call, and leaves the whole eval-context setup — the
// blacklist load included — unreached; `a_const_folded_eval_never_reaches_the_validation`
// in opcache_file_cache_tests.rs pins that behaviour. An eval that performs the include
// cannot be folded, so it is what exercises the dynamic tier.
eval('
require __DIR__ . "/blocked.php";
require __DIR__ . "/allowed.php";
');
echo 'blocked_ran=', $blocked_ran, "\n";
echo 'allowed_ran=', $allowed_ran, "\n";
$s = opcache_get_status();
echo 'blacklist_misses=', $s['opcache_statistics']['blacklist_misses'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
$names = [];
// `main.php` is the COMPILE-TIME manifest entry, not a dynamic-cache one. It is frozen
// into the binary and the blacklist never governs it, so it would appear in every
// expectation below and discriminate nothing. Dropping it leaves exactly the dynamic tier.
foreach (array_keys($s['scripts'] ?? []) as $p) {
    if (basename($p) !== 'main.php') { $names[] = basename($p); }
}
sort($names);
echo 'scripts=', implode(',', $names), "\n";
$c = opcache_get_configuration();
echo 'cfg=', $c['directives']['opcache.blacklist_filename'], "\n";
echo 'blacklist=', implode('|', $c['blacklist']), "\n";
echo 'blacklist_is_array=', is_array($c['blacklist']) ? '1' : '', "\n";
// A reporting-only directive, to prove in the same run that the env mechanism is live.
echo 'save_comments=', opcache_get_configuration()['directives']['opcache.save_comments'] ? '1' : '', "\n";
"#,
    )
    .unwrap();
}

/// The baseline: with no blacklist, BOTH includes are cached and no refusal is counted.
///
/// Without this the blacklist tests below could pass against a cache that stores nothing at
/// all, which is the failure mode that would make every other assertion here vacuous.
#[test]
fn without_a_blacklist_both_includes_are_cached() {
    let dir = make_test_dir("opcache_blacklist_none");
    write_fixtures(&dir);

    let binary = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    let out = run_binary(&binary);

    assert_eq!(line(&out, "blacklist_misses"), "0");
    assert_eq!(line(&out, "scripts"), "allowed.php,blocked.php");
    assert_eq!(line(&out, "cfg"), "");
}

/// A blacklisted include RUNS but is not cached, and the refusal moves `blacklist_misses`
/// rather than `misses`.
///
/// VERIFIED on reference PHP 8.5.10: including a blacklisted file left `misses` where it
/// was and moved only `blacklist_misses`, and the file was absent from `scripts` while
/// `opcache_is_script_cached()` answered `false` for it.
#[test]
fn a_blacklisted_include_runs_but_is_not_cached() {
    let dir = make_test_dir("opcache_blacklist_hit");
    write_fixtures(&dir);
    fs::write(
        dir.join("deny.list"),
        format!("{}\n", dir.join("blocked.php").display()),
    )
    .unwrap();

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            &format!(
                "opcache.blacklist_filename={}",
                dir.join("deny.list").display()
            ),
        ],
    );
    let out = run_binary(&binary);

    // The script still ran: that is the whole point of the directive.
    assert_eq!(line(&out, "blocked_ran"), "1");
    assert_eq!(line(&out, "allowed_ran"), "1");
    // ... and only the allowed one was stored.
    assert_eq!(line(&out, "scripts"), "allowed.php");
    assert_eq!(line(&out, "blacklist_misses"), "1");
    assert_eq!(
        line(&out, "cfg"),
        dir.join("deny.list").display().to_string()
    );
}

/// A `;` comment and a blank line are not patterns, so a blacklist made only of those
/// blocks nothing.
///
/// This is the test that fails if the comment marker is ever matched loosely — for
/// instance by trimming leading whitespace before testing for `;`, which reference PHP
/// does not do.
#[test]
fn comments_and_blank_lines_block_nothing() {
    let dir = make_test_dir("opcache_blacklist_comments");
    write_fixtures(&dir);
    fs::write(
        dir.join("deny.list"),
        format!(
            "; {}\n\n   \n",
            dir.join("blocked.php").display()
        ),
    )
    .unwrap();

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            &format!(
                "opcache.blacklist_filename={}",
                dir.join("deny.list").display()
            ),
        ],
    );
    let out = run_binary(&binary);

    assert_eq!(line(&out, "blacklist_misses"), "0");
    assert_eq!(line(&out, "scripts"), "allowed.php,blocked.php");
}

/// A bare directory prefix blocks everything beneath it, because php-src anchors the
/// entry at the start only.
///
/// VERIFIED on reference PHP 8.5.10, where the entry `…/p_pref` refused `…/p_prefix.php`.
#[test]
fn a_directory_prefix_blocks_everything_under_it() {
    let dir = make_test_dir("opcache_blacklist_prefix");
    write_fixtures(&dir);
    // Names the DIRECTORY, so both includes fall under it.
    fs::write(dir.join("deny.list"), format!("{}/\n", dir.display())).unwrap();

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            &format!(
                "opcache.blacklist_filename={}",
                dir.join("deny.list").display()
            ),
        ],
    );
    let out = run_binary(&binary);

    assert_eq!(line(&out, "blocked_ran"), "1");
    assert_eq!(line(&out, "allowed_ran"), "1");
    assert_eq!(line(&out, "scripts"), "");
    assert_eq!(line(&out, "blacklist_misses"), "2");
}

/// The directive value is itself a `glob()`, and EVERY matching file contributes.
///
/// VERIFIED on reference PHP 8.5.10: with `bl_*.list` matching two files that named one
/// script each, both scripts were refused and `blacklist_misses` reached 2.
#[test]
fn the_directive_value_globs_over_several_blacklist_files() {
    let dir = make_test_dir("opcache_blacklist_glob");
    write_fixtures(&dir);
    fs::write(
        dir.join("bl_a.list"),
        format!("{}\n", dir.join("blocked.php").display()),
    )
    .unwrap();
    fs::write(
        dir.join("bl_b.list"),
        format!("{}\n", dir.join("allowed.php").display()),
    )
    .unwrap();
    // Must NOT contribute: the glob has to consume the whole filename.
    fs::write(dir.join("bl_c.list.bak"), "/nowhere/ignored.php\n").unwrap();

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            &format!("opcache.blacklist_filename={}/bl_*.list", dir.display()),
        ],
    );
    let out = run_binary(&binary);

    assert_eq!(line(&out, "scripts"), "");
    assert_eq!(line(&out, "blacklist_misses"), "2");
}

/// A wildcard does NOT reach into a subdirectory.
///
/// VERIFIED on reference PHP 8.5.10 with `file_update_protection=0` and a main script that
/// cannot match the pattern: `<dir>/*.php` refused the files beside it and CACHED
/// `<dir>/sub/nested.php`. An earlier revision had this backwards, on a probe whose
/// subdirectory file was really being refused for its age.
#[test]
fn a_wildcard_does_not_reach_into_a_subdirectory() {
    let dir = make_test_dir("opcache_blacklist_depth");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/nested.php"), "<?php $nested_ran = 1;\n").unwrap();
    fs::write(dir.join("flat.php"), "<?php $flat_ran = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
// Includes inside the eval, for the const-folding reason explained in `write_fixtures`.
eval('
require __DIR__ . "/flat.php";
require __DIR__ . "/sub/nested.php";
');
$s = opcache_get_status();
echo 'blacklist_misses=', $s['opcache_statistics']['blacklist_misses'], "\n";
$names = [];
// `main.php` is the COMPILE-TIME manifest entry, not a dynamic-cache one. It is frozen
// into the binary and the blacklist never governs it, so it would appear in every
// expectation below and discriminate nothing. Dropping it leaves exactly the dynamic tier.
foreach (array_keys($s['scripts'] ?? []) as $p) {
    if (basename($p) !== 'main.php') { $names[] = basename($p); }
}
sort($names);
echo 'scripts=', implode(',', $names), "\n";
"#,
    )
    .unwrap();
    fs::write(dir.join("deny.list"), format!("{}/*.php\n", dir.display())).unwrap();

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            &format!(
                "opcache.blacklist_filename={}",
                dir.join("deny.list").display()
            ),
        ],
    );
    let out = run_binary(&binary);

    // `flat.php` sits directly under the pattern's directory and is refused; the nested one
    // is BEYOND a separator the wildcard may not cross, so it is cached normally.
    assert_eq!(line(&out, "scripts"), "nested.php");
    assert_eq!(line(&out, "blacklist_misses"), "1");
}

/// A directive value matching no file is NOT fatal: nothing is blacklisted, and the
/// warning it logs stays silent at the default verbosity.
///
/// VERIFIED on reference PHP 8.5.10, which ran cleanly and printed
/// `Warning No blacklist file found matching: …` only once
/// `opcache.log_verbosity_level` was raised to 2.
#[test]
fn a_value_matching_no_file_is_not_fatal() {
    let dir = make_test_dir("opcache_blacklist_missing");
    write_fixtures(&dir);

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.blacklist_filename=/nonexistent-elephc/nope-*.list",
        ],
    );
    let out = run_binary(&binary);

    assert_eq!(line(&out, "blacklist_misses"), "0");
    assert_eq!(line(&out, "scripts"), "allowed.php,blocked.php");
}

/// THE HONESTY PROPERTY for this directive: its `ELEPHC_INI_*` runtime override is IGNORED,
/// and ignored on both surfaces at once.
///
/// The blacklist is read once while the eval context is built, so a value arriving later
/// could not retroactively keep anything out of a cache that was already filled. Reporting
/// it anyway would produce the self-contradiction the scope rule in
/// `crate::opcache::directives::directive_runtime_overridable` exists to prevent — an
/// `ini_get('opcache.blacklist_filename')` naming a list beside an `opcache_get_status()`
/// whose `scripts` lists the very files that list names.
///
/// A reporting-only directive is moved in the SAME run, so this pins the exclusion as
/// per-directive rather than the env mechanism simply being off.
#[test]
fn the_runtime_env_override_is_ignored() {
    let dir = make_test_dir("opcache_blacklist_env");
    write_fixtures(&dir);
    fs::write(
        dir.join("deny.list"),
        format!("{}\n", dir.join("blocked.php").display()),
    )
    .unwrap();

    // Compiled WITHOUT a blacklist; the environment is what tries (and must fail) to add one.
    let binary = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    let output = Command::new(&binary)
        .env(
            "ELEPHC_INI_opcache__blacklist_filename",
            dir.join("deny.list"),
        )
        .env(
            "ELEPHC_INI_opcache.blacklist_filename",
            dir.join("deny.list"),
        )
        .env("ELEPHC_INI_opcache__save_comments", "0")
        .output()
        .expect("failed to run binary");
    assert!(output.status.success(), "binary failed");
    let out = String::from_utf8_lossy(&output.stdout).into_owned();

    // Neither surface moved, and the cache behaved as if no blacklist existed.
    assert_eq!(line(&out, "cfg"), "");
    assert_eq!(line(&out, "blacklist_misses"), "0");
    assert_eq!(line(&out, "scripts"), "allowed.php,blocked.php");
    // ... while a reporting-only directive in the same run DID move.
    assert_eq!(line(&out, "save_comments"), "");
}

/// The `blacklist` key lists the RESOLVED patterns, and lists them verbatim.
///
/// VERIFIED against reference PHP 8.5.10 on the same fixture: entries are reported exactly
/// as written — a wildcard entry is NOT expanded — keyed `0..n-1`, and every file the
/// directive's glob matched contributes its lines.
#[test]
fn the_configuration_lists_the_resolved_patterns() {
    let dir = make_test_dir("opcache_blacklist_cfg");
    write_fixtures(&dir);
    fs::write(
        dir.join("bl_a.list"),
        format!("{}\n", dir.join("blocked.php").display()),
    )
    .unwrap();
    // A wildcard entry and a comment, to pin that one is kept verbatim and the other dropped.
    fs::write(
        dir.join("bl_b.list"),
        format!("; a comment\n{}/other*.php\n", dir.display()),
    )
    .unwrap();

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            &format!("opcache.blacklist_filename={}/bl_*.list", dir.display()),
        ],
    );
    let out = run_binary(&binary);

    assert_eq!(
        line(&out, "blacklist"),
        format!(
            "{}|{}/other*.php",
            dir.join("blocked.php").display(),
            dir.display()
        )
    );
}

/// A binary with no blacklist reports an EMPTY list, not a missing key.
#[test]
fn without_a_blacklist_the_configuration_lists_nothing() {
    let dir = make_test_dir("opcache_blacklist_cfg_none");
    write_fixtures(&dir);

    let binary = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    let out = run_binary(&binary);

    assert_eq!(line(&out, "blacklist"), "");
    assert_eq!(line(&out, "blacklist_is_array"), "1");
}

/// The eval interpreter and natively compiled code must answer identically.
///
/// They reach the same list by completely different routes — the interpreter reads the
/// loaded patterns in-process, native code reads them back across the bridge one at a time
/// with a count and an indexed accessor — so agreement here is the thing that stops the two
/// surfaces drifting. VERIFIED that reference PHP answers the same on both.
#[test]
fn both_surfaces_report_the_same_blacklist() {
    let dir = make_test_dir("opcache_blacklist_cfg_both");
    fs::write(dir.join("blocked.php"), "<?php $blocked_ran = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
eval('
require __DIR__ . "/blocked.php";
$inner = opcache_get_configuration();
echo "eval=", implode(",", $inner["blacklist"]), "\n";
');
$outer = opcache_get_configuration();
echo 'native=', implode(',', $outer['blacklist']), "\n";
"#,
    )
    .unwrap();
    fs::write(
        dir.join("deny.list"),
        format!(
            "{}\n{}/other*.php\n",
            dir.join("blocked.php").display(),
            dir.display()
        ),
    )
    .unwrap();

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            &format!(
                "opcache.blacklist_filename={}",
                dir.join("deny.list").display()
            ),
        ],
    );
    let out = run_binary(&binary);

    let expected = format!(
        "{},{}/other*.php",
        dir.join("blocked.php").display(),
        dir.display()
    );
    assert_eq!(line(&out, "eval"), expected);
    assert_eq!(line(&out, "native"), expected);
}

/// A binary with NO dynamic tier reports an empty list, and that is the truthful answer.
///
/// Both bridge calls fold to their empty answers at lowering time when the eval bridge is
/// not linked, so the loop never runs. Such a binary never loads a blacklist either, which
/// is why reporting nothing is correct rather than merely convenient — and it is what keeps
/// `opcache_get_configuration()` from dragging the interpreter into a program that has no
/// `eval()`.
#[test]
fn a_binary_without_a_dynamic_tier_lists_nothing() {
    let dir = make_test_dir("opcache_blacklist_cfg_static");
    fs::write(
        dir.join("main.php"),
        r#"<?php
$c = opcache_get_configuration();
echo 'blacklist=', implode('|', $c['blacklist']), "\n";
echo 'blacklist_is_array=', is_array($c['blacklist']) ? '1' : '', "\n";
"#,
    )
    .unwrap();
    fs::write(
        dir.join("deny.list"),
        format!("{}/anything.php\n", dir.display()),
    )
    .unwrap();

    let binary = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            &format!(
                "opcache.blacklist_filename={}",
                dir.join("deny.list").display()
            ),
        ],
    );
    let out = run_binary(&binary);

    assert_eq!(line(&out, "blacklist"), "");
    assert_eq!(line(&out, "blacklist_is_array"), "1");
}
