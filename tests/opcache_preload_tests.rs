//! Purpose:
//! End-to-end tests for `opcache.preload` / `opcache_get_status()['preload_statistics']`, the
//! compile-time preload verdict baked by `src/opcache_prelude.rs` and enforced by
//! `src/pipeline.rs`. Covers all four reference rows: the default (no `preload_statistics` key
//! at all), a resolvable preload file (the statistics block, with and without the
//! outside-the-manifest warning), an unresolvable path with the cache enabled (a COMPILE ERROR,
//! the AOT equivalent of reference PHP's startup fatal), and a set directive with the cache
//! disabled (nothing happens — not even path validation).
//!
//! Called from:
//! - `cargo test --test opcache_preload_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation here is PINNED FROM REFERENCE PHP 8.5.6 (Homebrew, `Zend OPcache`
//!   loaded), reproduced with
//!   `php -d opcache.enable=1 -d opcache.enable_cli=1 -d opcache.preload=<file> -r
//!   'var_export(opcache_get_status());'`. The verified reference shape is: `preload_statistics`
//!   sits BETWEEN `opcache_statistics` and `scripts` in the top-level array, and its keys are
//!   `memory_consumption` (int), `functions` (list<string>), `classes` (list<string>),
//!   `scripts` (list<string>) IN THAT ORDER — with `functions`/`classes` OMITTED ENTIRELY when
//!   empty rather than reported as empty arrays. Nothing else is added to the top level.
//! - The reference startup fatal for a missing preload file was verified too:
//!   `PHP Fatal error:  Failed opening required '<path>' … in Unknown on line 0`, exit 1, before
//!   a single line of the script runs. Because elephc fixes its INI at build time, that becomes
//!   a compile error — and, like reference's fatal, it fires whether or not the program ever
//!   calls an OPcache function.
//! - The cache-disabled row was verified too: `-d opcache.enable_cli=0 -d opcache.preload=<missing>`
//!   runs cleanly and exits 0, and `opcache_get_status()` returns `false`. So elephc must not
//!   validate the path in that state either.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   the same harness style as `opcache_restrict_api_tests` / `opcache_ini_tests`. Host-target
//!   only (macOS aarch64 local).
//! - The probe uses `count()` and `isset()` rather than `array_keys()` / `array_key_exists()`:
//!   only the former two narrow through elephc's `is_array()` guard on the `array|false` return
//!   today (the latter two are rejected with "argument must be array"). That is a pre-existing
//!   checker limitation unrelated to preloading; the probe works around it rather than pinning it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// The probe program. It declares one function, one class and one interface so the
/// `functions`/`classes` lists have something real to report, then dumps the discriminating
/// facts: the top-level key COUNT (9 without preloading, 10 with — the single added key), and,
/// when present, the block's own key count and every field.
const PROBE: &str = r#"<?php
function probe_helper() { return 1; }
class ProbeWidget {}
interface ProbeIface {}
$s = opcache_get_status();
if (is_array($s)) {
    echo 'count=', count($s), "\n";
    if (isset($s['preload_statistics'])) {
        $p = $s['preload_statistics'];
        echo 'pcount=', count($p), "\n";
        echo 'mem=', ($p['memory_consumption'] > 0 ? 'POSITIVE' : 'NONPOSITIVE'), "\n";
        echo 'fns=', implode(',', $p['functions']), "\n";
        echo 'cls=', implode(',', $p['classes']), "\n";
        echo 'scr=', implode(',', $p['scripts']), "\n";
    } else {
        echo "preload=absent\n";
    }
} else {
    echo "status=false\n";
}
"#;

/// Creates an isolated temp dir unique across parallel test threads/processes, returned
/// CANONICALIZED so a preload path built from it matches the spelling elephc resolves to (on
/// macOS `std::env::temp_dir()` lives under `/var/folders/...`, which resolves to
/// `/private/var/folders/...`).
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
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

/// Writes `PROBE` into `dir` and runs the compiler over it with the supplied `--ini`
/// assignments, returning `(success, stdout, stderr, executable path)` WITHOUT asserting — the
/// missing-preload row needs the failure, so the assertion belongs to each test.
fn try_compile(dir: &Path, stem: &str, ini: &[String]) -> (bool, String, String, PathBuf) {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, PROBE).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    for assignment in ini {
        cmd.arg("--ini").arg(assignment);
    }
    let output = cmd.output().expect("failed to spawn elephc");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        dir.join(stem),
    )
}

/// Compiles `PROBE`, asserting success, and returns `(compiler stderr, executable path)`. The
/// compiler stderr is returned because the outside-the-manifest WARNING is emitted there.
fn compile(dir: &Path, stem: &str, ini: &[String]) -> (String, PathBuf) {
    let (ok, out, err, bin) = try_compile(dir, stem, ini);
    assert!(ok, "elephc compile failed for {ini:?}:\n{out}\n{err}");
    (elephc_diagnostics(&err), bin)
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` reports the static-`getaddrinfo`/`gethostbyname` glibc
/// notes and the `.note.GNU-stack` deprecation, while Apple's linker stays silent. Those lines
/// start with `/usr/bin/ld:` or a `(.text.…)` section reference, so anchoring on elephc's own
/// line starts isolates its diagnostics — and still surfaces an UNEXPECTED elephc warning, which
/// an allow-list of known messages would have hidden.
///
/// elephc emits two prefixes: `Warning: …` for the INI-override diagnostics (`src/main.rs`) and
/// `warning: …` / `warning[line:col]: …` for compile warnings (`src/errors/report.rs`), the
/// latter being how the outside-the-manifest preload warning arrives.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs a compiled executable and returns its stdout, asserting a clean exit.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Writes `source` into `dir` and compiles it, asserting success. The preload-usage tests need
/// their own program rather than `PROBE`, because what they check is what the ENTRY script can
/// see — not what `opcache_get_status()` reports about it.
fn compile_source(dir: &Path, stem: &str, source: &str, ini: &[String]) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    for assignment in ini {
        cmd.arg("--ini").arg(assignment);
    }
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed for {ini:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// The PHP source a preload file declares one of every kind in, plus a top-level side effect.
const PRELOAD_LIB: &str = r#"<?php
function preloaded_helper(): string { return "from preload"; }
class PreloadedClass { public function hi(): string { return "hi from preload"; } }
interface PreloadedInterface {}
trait PreloadedTrait {}
enum PreloadedEnum: string { case A = 'a'; }
echo "PRELOAD FILE RAN\n";
"#;

/// The entry script asking what it can see, WITHOUT including the preload file.
const PRELOAD_USER: &str = r#"<?php
echo "function  ", var_export(function_exists('preloaded_helper'), true), "
";
echo "class     ", var_export(class_exists('PreloadedClass', false), true), "
";
echo "interface ", var_export(interface_exists('PreloadedInterface', false), true), "
";
echo "trait     ", var_export(trait_exists('PreloadedTrait', false), true), "
";
echo "enum      ", var_export(enum_exists('PreloadedEnum', false), true), "
";
echo "call      ", preloaded_helper(), "
";
echo "method    ", (new PreloadedClass)->hi(), "
";
"#;

/// THE CAPABILITY: a preloaded file's declarations are available to the entry script without it
/// including them, and the preload file's own top-level code runs FIRST.
///
/// Every line of the expected output is what reference PHP 8.5.6 printed for the same two files:
/// `php -n -d opcache.enable=1 -d opcache.enable_cli=1 -d opcache.preload=lib.php user.php`.
/// Before this, elephc resolved the directive and reported statistics about it but never
/// compiled the file in, so `preloaded_helper()` was an unknown function and the build failed.
#[test]
fn preloaded_declarations_are_usable_without_including_the_file() {
    let dir = make_test_dir("opcache_preload_usable");
    let lib = dir.join("lib.php");
    fs::write(&lib, PRELOAD_LIB).unwrap();
    let bin = compile_source(
        &dir,
        "user",
        PRELOAD_USER,
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", lib.display()),
        ],
    );

    assert_eq!(
        run_binary(&bin),
        "PRELOAD FILE RAN\n\
         function  true\n\
         class     true\n\
         interface true\n\
         trait     true\n\
         enum      true\n\
         call      from preload\n\
         method    hi from preload\n"
    );
}

/// A preload file's own `require_once` preloads TRANSITIVELY: reference PHP reports both files in
/// `preload_statistics.scripts` and makes both files' symbols available (VERIFIED on 8.5.6 with a
/// preload file requiring one dependency). elephc gets this for free — the resolver inlines the
/// preload file, and inlining it walks into its own includes.
#[test]
fn preloading_is_transitive_through_the_preload_files_own_requires() {
    let dir = make_test_dir("opcache_preload_transitive");
    fs::write(
        dir.join("dep.php"),
        "<?php\nfunction dep_helper(): string { return \"from dep\"; }\nclass DepClass {}\n",
    )
    .unwrap();
    let lib = dir.join("lib.php");
    fs::write(
        &lib,
        "<?php\nrequire_once __DIR__ . '/dep.php';\nfunction lib_helper(): int { return 1; }\n",
    )
    .unwrap();

    let bin = compile_source(
        &dir,
        "user",
        r#"<?php
echo "dep_fn    ", var_export(function_exists('dep_helper'), true), "
";
echo "dep_class ", var_export(class_exists('DepClass', false), true), "
";
echo "lib_fn    ", var_export(function_exists('lib_helper'), true), "
";
echo "call      ", dep_helper(), "
";
"#,
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", lib.display()),
        ],
    );

    assert_eq!(
        run_binary(&bin),
        "dep_fn    true\n\
         dep_class true\n\
         lib_fn    true\n\
         call      from dep\n"
    );
}

/// With the cache DISABLED — the CLI default — `opcache.preload` is not consulted at all, so the
/// file is NOT compiled in and its symbols stay unknown. Reference PHP agrees: with
/// `opcache.enable_cli=0` every `*_exists()` on a preload-file symbol answers `false` and the
/// preload file's top-level code never runs (VERIFIED).
#[test]
fn a_disabled_cache_does_not_compile_the_preload_file_in() {
    let dir = make_test_dir("opcache_preload_disabled_syms");
    let lib = dir.join("lib.php");
    fs::write(&lib, PRELOAD_LIB).unwrap();
    let bin = compile_source(
        &dir,
        "user",
        r#"<?php
echo "function  ", var_export(function_exists('preloaded_helper'), true), "
";
echo "class     ", var_export(class_exists('PreloadedClass', false), true), "
";
"#,
        &[format!("opcache.preload={}", lib.display())],
    );

    assert_eq!(run_binary(&bin), "function  false\nclass     false\n");
}

/// THE BASELINE: with the cache enabled but `opcache.preload` at its default (empty), the status
/// array carries NO `preload_statistics` key and its top-level key count is the unchanged 9 —
/// the same figure `opcache_restrict_api_tests` pins as `ARRAY9`. Reference PHP agrees: an
/// unset (or explicitly empty) `opcache.preload` produces no such key.
#[test]
fn default_has_no_preload_statistics() {
    let dir = make_test_dir("opcache_preload_default");
    let (err, bin) = compile(&dir, "app", &["opcache.enable_cli=1".to_string()]);
    assert_eq!(err, "", "the default path must emit no diagnostics: {err:?}");
    assert_eq!(run_binary(&bin), "count=9\npreload=absent\n");
}

/// A preload file that IS the entry script (and therefore a member of the compile-time script
/// manifest) emits the statistics block SILENTLY: exactly one key is added to the top level, and
/// the block carries the four reference keys with the binary's real symbols and manifest paths.
#[test]
fn preloading_the_entry_file_emits_statistics_silently() {
    let dir = make_test_dir("opcache_preload_entry");
    let entry = dir.join("app.php");
    let (err, bin) = compile(
        &dir,
        "app",
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", entry.display()),
        ],
    );
    assert_eq!(
        err, "",
        "a manifest-member preload file must warn about nothing: {err:?}"
    );

    let out = run_binary(&bin);
    // Exactly ONE key added to the top level (9 → 10): reference adds `preload_statistics` and
    // nothing else.
    assert!(out.contains("count=10\n"), "{out}");
    // The four reference keys: memory_consumption, functions, classes, scripts.
    assert!(out.contains("pcount=4\n"), "{out}");
    assert!(out.contains("mem=POSITIVE\n"), "{out}");
    // REAL user symbols, not a fabricated or empty interim. The interface lands under `classes`,
    // as reference PHP does.
    assert!(out.contains("fns=probe_helper\n"), "{out}");
    assert!(out.contains("cls=ProbeWidget,ProbeIface\n"), "{out}");
    // `scripts` is the compile-time manifest: the canonicalized entry file.
    assert!(
        out.contains(&format!("scr={}\n", entry.display())),
        "scripts must report the canonical manifest path:\n{out}"
    );
    // No compiler prelude leaked into the SYMBOL lists (checked on those two lines only: the
    // `scr=` line legitimately carries the temp dir's name, which contains "opcache_").
    for line in out.lines().filter(|l| l.starts_with("fns=") || l.starts_with("cls=")) {
        assert!(!line.contains("opcache_"), "prelude leaked into symbols: {line}");
        assert!(!line.contains("var_export"), "prelude leaked into symbols: {line}");
        assert!(!line.contains("__elephc"), "prelude leaked into symbols: {line}");
    }
}

/// Preloading inserts reference PHP's synthetic `$PRELOAD$` entry, and `num_cached_scripts`
/// counts it.
///
/// VERIFIED on reference PHP 8.5.10, preloading a file that pulls in one dependency:
///
/// ```text
/// scripts keys: preloader.php, $PRELOAD$, entry.php, lib.php
/// $PRELOAD$  => full_path '$PRELOAD$', hits 0, memory_consumption 38488,
///               last_used 'Thu Jan  1 01:00:00 1970', last_used_timestamp 0,
///               timestamp 0, revalidate 0
/// preload_statistics.memory_consumption = 38488      <- the SAME figure
/// num_cached_scripts = 4                             <- three real scripts plus the marker
/// ```
///
/// The key is NOT a path: a caller walking `scripts` meets it among real filenames, and
/// `file_exists('$PRELOAD$')` is false. An earlier revision deliberately omitted it, on the
/// grounds that an elephc binary allocates no shared-memory block for it to stand for. That
/// argument did not survive what the surface already reports: `preload_statistics.memory_consumption`
/// is already a synthetic figure over the same manifest, and `scripts` already reports the
/// manifest as if it were a cache — the repo's own "the binary IS the cache" premise, under
/// which the block the marker stands for is real here too.
///
/// Its POSITION is not asserted. Reference puts it second, which is hash insertion order rather
/// than a contract, and elephc's `scripts` is manifest order — already a different order from
/// reference's.
#[test]
fn preloading_inserts_the_synthetic_preload_entry() {
    let dir = make_test_dir("opcache_preload_marker");
    let lib = dir.join("lib.php");
    fs::write(&lib, PRELOAD_LIB).unwrap();
    let bin = compile_source(
        &dir,
        "app",
        r#"<?php
$s = opcache_get_status();
$scripts = $s['scripts'];
echo 'has_marker=', array_key_exists('$PRELOAD$', $scripts) ? '1' : '', "\n";
$m = $scripts['$PRELOAD$'] ?? [];
echo 'full_path=', $m['full_path'] ?? '', "\n";
echo 'hits=', $m['hits'] ?? '', "\n";
echo 'mem=', $m['memory_consumption'] ?? '', "\n";
echo 'last_used_timestamp=', $m['last_used_timestamp'] ?? '', "\n";
echo 'timestamp=', $m['timestamp'] ?? '', "\n";
echo 'revalidate=', $m['revalidate'] ?? '', "\n";
echo 'preload_mem=', $s['preload_statistics']['memory_consumption'], "\n";
echo 'num_cached_scripts=', $s['opcache_statistics']['num_cached_scripts'], "\n";
echo 'real_scripts=', count($scripts) - 1, "\n";
"#,
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", lib.display()),
        ],
    );

    let out = run_binary(&bin);
    let line = |key: &str| -> String {
        out.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .unwrap_or_else(|| panic!("missing `{key}=` in:\n{out}"))
            .to_string()
    };

    assert_eq!(line("has_marker"), "1", "{out}");
    assert_eq!(line("full_path"), "$PRELOAD$");
    assert_eq!(line("hits"), "0");
    // The marker's memory is the preload block's, to the byte.
    assert_eq!(line("mem"), line("preload_mem"));
    // Every clock is zero: it stands for a block, not a file there is anything to stat.
    assert_eq!(line("last_used_timestamp"), "0");
    assert_eq!(line("timestamp"), "0");
    assert_eq!(line("revalidate"), "0");
    // Counted, exactly once, on top of the real manifest entries.
    let real: i64 = line("real_scripts").parse().unwrap();
    assert_eq!(
        line("num_cached_scripts"),
        (real + 1).to_string(),
        "the marker must be counted once:\n{out}"
    );
}

/// WITHOUT preloading there is no marker, and the count is the manifest alone.
///
/// The companion to the test above: it is what fails if the entry is ever inserted
/// unconditionally.
#[test]
fn without_preloading_there_is_no_synthetic_entry() {
    let dir = make_test_dir("opcache_preload_no_marker");
    let bin = compile_source(
        &dir,
        "app",
        r#"<?php
$s = opcache_get_status();
echo 'has_marker=', array_key_exists('$PRELOAD$', $s['scripts']) ? '1' : '', "\n";
echo 'num_cached_scripts=', $s['opcache_statistics']['num_cached_scripts'], "\n";
echo 'count=', count($s['scripts']), "\n";
echo 'preload=', array_key_exists('preload_statistics', $s) ? 'present' : 'absent', "\n";
"#,
        &["opcache.enable_cli=1".to_string()],
    );

    let out = run_binary(&bin);
    assert!(out.contains("has_marker=\n"), "{out}");
    assert!(out.contains("preload=absent\n"), "{out}");
    assert!(out.contains("num_cached_scripts=1\n"), "{out}");
    assert!(out.contains("count=1\n"), "{out}");
}

/// A preload file the entry script never mentions is COMPILED IN, silently, and its symbols
/// become part of the binary — which is what preloading MEANS.
///
/// Reference PHP 8.5.6, VERIFIED: with `opcache.preload` naming a file that declares one of each
/// kind, the entry script sees `function_exists`, `class_exists`, `interface_exists`,
/// `trait_exists` and `enum_exists` all answer `true` without including it, and the preload
/// file's own top-level output appears FIRST. elephc reaches the same place by injecting an
/// implicit `require_once` ahead of the entry program, so the resolver inlines the file.
///
/// This test replaced one that pinned the opposite: elephc used to WARN that the file was "not in
/// this binary's compile-time OPcache script manifest, so it is not compiled into the binary".
/// That warning described a limitation, and the limitation is gone — the file is always a
/// manifest member now, so the warning became unreachable and was removed with it.
#[test]
fn preload_outside_manifest_is_compiled_in_silently() {
    let dir = make_test_dir("opcache_preload_outside");
    let other = dir.join("other.php");
    fs::write(
        &other,
        "<?php\nfunction preloaded_only() { return 7; }\nclass PreloadedOnly {}\n",
    )
    .unwrap();
    let (err, bin) = compile(
        &dir,
        "app",
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", other.display()),
        ],
    );

    assert_eq!(
        err, "",
        "compiling a preload file in must diagnose nothing: {err:?}"
    );
    let out = run_binary(&bin);
    assert!(out.contains("count=10\n"), "{out}");
    assert!(out.contains("pcount=4\n"), "{out}");
    // The preload file's own symbols are now part of the binary, beside the probe's own, and
    // under their PHP names — the resolver renames a function it inlines out of an include to
    // `__elephc_include_variant_<hash>_<name>`, which must never reach this list.
    assert!(
        out.contains("fns=preloaded_only,probe_helper\n"),
        "the preloaded function must be reported, under its PHP name:\n{out}"
    );
    assert!(
        out.contains("cls=PreloadedOnly,ProbeWidget,ProbeIface\n"),
        "the preloaded class must be reported:\n{out}"
    );
    // And `scripts` carries the preload file, as reference's `preload_statistics.scripts` does.
    assert!(
        out.contains(&other.display().to_string()),
        "the preload file must be a manifest member:\n{out}"
    );
}

/// CACHE ENABLED + UNRESOLVABLE PATH: a hard COMPILE ERROR naming the directive and the path.
/// This is the AOT equivalent of reference PHP's startup fatal `Failed opening required '<path>'`,
/// and like that fatal it does not depend on the program calling any OPcache function.
#[test]
fn missing_preload_file_fails_compilation() {
    let dir = make_test_dir("opcache_preload_missing");
    let missing = dir.join("nope.php");
    let (ok, _out, err, _bin) = try_compile(
        &dir,
        "app",
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", missing.display()),
        ],
    );

    assert!(!ok, "a missing preload file must fail the build: {err:?}");
    assert!(err.contains("opcache.preload:"), "{err:?}");
    assert!(
        err.contains(&missing.display().to_string()),
        "the error must name the unresolvable path: {err:?}"
    );
    assert!(
        err.contains("failed opening required"),
        "the error must echo reference's fatal wording: {err:?}"
    );
    // The binary must not exist: nothing is shipped for an unresolvable preload.
    assert!(!dir.join("app").exists(), "no binary may be produced");
}

/// CACHE DISABLED: a set `opcache.preload` is ignored ENTIRELY. The default CLI binary has
/// `opcache.enable_cli=0`, so even a MISSING preload path compiles cleanly, runs, and reports
/// `opcache_get_status() === false` — exactly what reference PHP does with
/// `-d opcache.enable_cli=0 -d opcache.preload=<missing>` (exit 0, nothing preloaded).
#[test]
fn disabled_cache_ignores_preload_entirely() {
    let dir = make_test_dir("opcache_preload_disabled");
    let missing = dir.join("nope.php");
    let (err, bin) = compile(
        &dir,
        "app",
        &[format!("opcache.preload={}", missing.display())],
    );
    assert_eq!(
        err, "",
        "a disabled cache must neither validate the path nor warn: {err:?}"
    );
    assert_eq!(run_binary(&bin), "status=false\n");
}

/// An explicitly EMPTY `--ini opcache.preload=` is the same as the default: no key, no
/// diagnostics. Pinned to reference PHP, where `-d opcache.preload=` reports no
/// `preload_statistics`.
#[test]
fn explicitly_empty_preload_matches_the_default() {
    let dir = make_test_dir("opcache_preload_empty");
    let (err, bin) = compile(
        &dir,
        "app",
        &[
            "opcache.enable_cli=1".to_string(),
            "opcache.preload=".to_string(),
        ],
    );
    assert_eq!(err, "", "{err:?}");
    assert_eq!(run_binary(&bin), "count=9\npreload=absent\n");
}
