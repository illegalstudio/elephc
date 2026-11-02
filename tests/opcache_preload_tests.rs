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

/// Runs a compiled preload fixture with one runtime environment condition.
fn run_binary_with_env(bin: &Path, name: &str, value: &str) -> String {
    let output = Command::new(bin)
        .env(name, value)
        .output()
        .expect("failed to run compiled binary");
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

/// Every `opcache.preload` given is a compile root, in order, not only the last: a binary that
/// must answer under more than one configuration (Symfony's `--env=prod` and `--env=dev`, each with
/// its own container preload) needs every root's declarations. A class both roots declare is
/// declared once, and only the LAST preload's top level runs at startup, as php runs the one
/// directive it was given. Before, the first preload was ignored: the dev container's classes were
/// not in the binary and `bin/console about --env=dev` died on an include it could not run.
#[test]
fn every_preload_directive_is_a_compile_root() {
    let dir = make_test_dir("opcache_preload_multiple");
    let first = dir.join("first.php");
    let second = dir.join("second.php");
    // A preload names what it loads through `require`, as Symfony's container dumps do.
    fs::write(
        dir.join("first_lib.php"),
        "<?php\nclass OnlyInFirst { public function hi(): string { return \"first\"; } }\nclass Shared {}\n",
    )
    .unwrap();
    fs::write(&first, "<?php\necho \"FIRST RAN\\n\";\nrequire __DIR__ . '/first_lib.php';\n").unwrap();
    fs::write(
        &second,
        "<?php\necho \"SECOND RAN\\n\";\nclass OnlyInSecond {}\nclass Shared {}\n",
    )
    .unwrap();
    let bin = compile_source(
        &dir,
        "user",
        "<?php\n$c = $argc > 9 ? 'X' : 'OnlyInFirst';\necho class_exists($c) ? (new $c())->hi() : 'missing', \"|\", class_exists('OnlyInSecond') ? 'second' : 'missing', \"|\", class_exists('Shared') ? 'shared' : 'missing', \"\\n\";\n",
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", first.display()),
            format!("opcache.preload={}", second.display()),
        ],
    );
    assert_eq!(run_binary(&bin), "SECOND RAN\nfirst|second|shared\n");
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

/// Verifies a top-level `return` in the preloaded file ends THAT FILE, not the program.
///
/// `return` at the top level of an included file is ordinary PHP — it is how a file says
/// "nothing further here" — and reference PHP treats it as ending the include, handing control
/// back to the caller. elephc inlines the preload's `require_once`, so before the rewrite in
/// `discard_first_include_return` the inlined `return` read as a return from `main`: the entry
/// script never ran and the process exited 0. A silent truncation with a zero exit status is
/// the worst shape a divergence can take, and nothing about writing `return` in a preload file
/// warns you about it.
///
/// VERIFIED against reference PHP 8.5: `opcache.preload` on a file that prints `A` then
/// returns, with an entry that prints `MAIN`, prints `AMAIN`.
///
/// The second half pins the value's EVALUATION. `return f();` discards the value here, but it
/// must still call `f()`; a rewrite that simply dropped the statement would pass the first
/// assertion and silently skip the call.
#[test]
fn a_top_level_return_in_the_preload_file_does_not_truncate_the_program() {
    let dir = make_test_dir("opcache_preload_return");
    fs::write(
        dir.join("lib.php"),
        "<?php\necho 'A';\nreturn;\necho 'NEVER';\n",
    )
    .unwrap();
    fs::write(dir.join("main.php"), "<?php\necho 'MAIN';\n").unwrap();

    let bin = compile_source(
        &dir,
        "main",
        "<?php\necho 'MAIN';\n",
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", dir.join("lib.php").display()),
        ],
    );
    assert_eq!(
        run_binary(&bin),
        "AMAIN",
        "the preload's top-level return swallowed the entry script"
    );

    // The discarded value is still evaluated.
    let dir = make_test_dir("opcache_preload_return_value");
    fs::write(
        dir.join("lib.php"),
        "<?php\necho 'X';\nreturn marker();\necho 'NEVER';\n",
    )
    .unwrap();
    let bin = compile_source(
        &dir,
        "main",
        "<?php\nfunction marker() { echo 'CALLED'; return 1; }\necho 'MAIN';\n",
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", dir.join("lib.php").display()),
        ],
    );
    assert_eq!(
        run_binary(&bin),
        "XCALLEDMAIN",
        "the returned expression was dropped instead of evaluated"
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

/// A file the preload script names with `opcache_compile_file()` is compiled, not run, and a
/// runtime include of it runs that compiled code: php's opcache splits a file the same way. Each
/// include re-runs the file (fresh objects, one `1`-free `return` value), `require_once` answers
/// `true` once the file ran, a path spelled with `..` reaches the same unit, and a file the
/// preload did NOT name still goes through the interpreter -- including one that reads the
/// includer's variable, which only an include in the caller's scope can see.
#[test]
fn opcache_compile_file_units_run_compiled_includes() {
    let dir = make_test_dir("opcache_preload_units");
    let write = |name: &str, content: &str| fs::write(dir.join(name), content).unwrap();
    write(
        "preload.php",
        "<?php\nopcache_compile_file(__DIR__ . '/routes.php');\nopcache_compile_file(__DIR__ . '/bundles.php');\n",
    );
    write(
        "routes.php",
        r#"<?php
return [
    false,
    ['/' => [[['_route' => 'home', '_controller' => 'App\\Controller::home'], null, ['GET' => 0], null, false]]],
    [0 => '{^(?|/greet/([^/]++)(*:25))/?$}sDu'],
    PHP_INT_SIZE * 2,
    \App\Thing::class,
];
"#,
    );
    write(
        "bundles.php",
        "<?php\nreturn ['First' => new \\App\\Thing('first'), 'Second' => new \\App\\Thing('second')];\n",
    );
    write("other.php", "<?php\nreturn ['other' => $seed ?? 'no seed'];\n");
    write(
        "main.php",
        r#"<?php
namespace App;
class Thing {
    public static int $built = 0;
    public function __construct(public string $name) { self::$built++; }
}
function load(string $file): mixed { $seed = 'from load'; return require $file; }
function loadOnce(string $file): mixed { return require_once $file; }
$dir = $argc > 5 ? '/nowhere' : __DIR__;
$routes = load("$dir/routes.php");
echo json_encode($routes), "\n";
echo load("$dir/../" . basename($dir) . "/routes.php") === $routes ? "equal" : "different", "\n";
$bundles = load("$dir/bundles.php");
echo implode(',', array_map(fn ($b) => $b->name, $bundles)), ' built=', Thing::$built, "\n";
$more = load("$dir/bundles.php");
echo $more['First'] === $bundles['First'] ? "same object" : "fresh objects", ' built=', Thing::$built, "\n";
var_export(loadOnce("$dir/routes.php"));
echo "\n";
var_export(loadOnce("$dir/other.php"));
echo "\n";
var_export(loadOnce("$dir/other.php"));
echo "\n";
echo json_encode(load("$dir/other.php")), "\n";
"#,
    );
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg(dir.join("main.php"));
    cmd.arg("--ini").arg("opcache.enable_cli=1");
    cmd.arg("--ini").arg(format!("opcache.preload={}", dir.join("preload.php").display()));
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let bin = dir.join("main");
    assert_eq!(
        run_binary(&bin),
        r#"[false,{"\/":[[{"_route":"home","_controller":"App\\Controller::home"},null,{"GET":0},null,false]]},["{^(?|\/greet\/([^\/]++)(*:25))\/?$}sDu"],16,"App\\Thing"]
equal
first,second built=2
fresh objects built=4
true
array (
  'other' => 'no seed',
)
true
{"other":"from load"}
"#
    );
    // Only `other.php` may reach the interpreter: two of its three includes run it (the second
    // `require_once` is answered without it), and none of the five unit includes may.
    let traced = Command::new(&bin)
        .env("ELEPHC_EVAL_TRACE", "ffi")
        .output()
        .expect("failed to run compiled binary");
    let bridge_includes = String::from_utf8_lossy(&traced.stderr)
        .lines()
        .filter(|line| line.contains("__elephc_eval_include") && !line.contains("__elephc_eval_include_"))
        .count();
    assert_eq!(bridge_includes, 2, "{}", String::from_utf8_lossy(&traced.stderr));
}

/// The first compiled-unit include of a program that links the eval bridge returns an intact array.
///
/// With the bridge every function carries the hidden `func_get_args` collector, the include unit
/// too, and the dispatch called the unit with the first argument register still holding the
/// canonical-path buffer it had just freed. The unit adopted it as its collector: acquired on entry,
/// released on exit -- after its own first `array_new` had reused that block. The released block
/// was the returned array, which the next allocation took over. Symfony's
/// `CompiledUrlMatcher` read its routes from it and answered "No route found" on every request.
///
/// Whether the freed block is the one the unit reuses depends on the path's length, so the same
/// file is preloaded from nine directories of growing depth. Measured with the fix reverted: six of
/// those nine lengths broke the first include when each was the only one in its program.
#[test]
fn a_compiled_unit_include_in_an_eval_program_keeps_its_returned_array() {
    let dir = make_test_dir("opcache_preload_unit_collector");
    let routes = r#"<?php
return [
    false,
    ['/' => [[['_route' => 'home'], null, ['GET' => 0], null, false, false, null]],
     '/plain' => [[['_route' => 'plain'], null, ['GET' => 0], null, false, false, null]]],
    [0 => '{^x$}'],
    [22 => [[['_route' => 'greet'], ['name'], ['GET' => 0], null, false, true, null]]],
    null,
];
"#;
    let mut preload = String::from("<?php\n");
    for depth in 0..9 {
        let sub = format!("d{}", "x".repeat(depth * 16));
        fs::create_dir_all(dir.join(&sub)).unwrap();
        fs::write(dir.join(&sub).join("routes.php"), routes).unwrap();
        preload.push_str(&format!("opcache_compile_file(__DIR__ . '/{sub}/routes.php');\n"));
    }
    fs::write(dir.join("preload.php"), preload).unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
class Matcher {
    public array $static = [];
    public function __construct(array $routes) { [, $x] = $routes; $this->static = $x; }
}
function load(string $file): array { return require $file; }
$base = $argc > 5 ? '/nowhere' : __DIR__;
foreach (range(0, 8) as $depth) {
    $m = new Matcher(load($base . '/d' . str_repeat('x', $depth * 16) . '/routes.php'));
    echo implode(',', array_keys($m->static)), "\n";
}
echo eval('return "e";'), "\n";
"#,
    )
    .unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg(dir.join("main.php"));
    cmd.arg("--ini").arg("opcache.enable_cli=1");
    cmd.arg("--ini").arg(format!("opcache.preload={}", dir.join("preload.php").display()));
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(run_binary(&dir.join("main")), format!("{}e\n", "/,/plain\n".repeat(9)));
}

/// PINS A DIVERGENCE, not parity: under the CLI, a global the preload assigns reaches the entry.
///
/// Reference preloads before the request exists and tears that scope down, so the entry script
/// reads the variable as NULL — MEASURED: `$from_preload ?? 'absent'` prints `absent`. elephc
/// inlines the preload's top-level code into `main` under the CLI, so the variable is still set
/// and it prints `SURVIVED`. The `--web` build already matches reference.
///
/// A fix exists and was reverted: wrapping the CLI guard in the web path's synthetic function
/// gave the preload its own scope and closed this, and broke every preload that declares a
/// function — the declaration is emitted as an include variant that nothing then defines, and
/// the link fails. See `inject_preload_require`'s docblock.
///
/// FLIP THIS TO PARITY when that is solved; do not delete it.
#[test]
fn a_preload_global_reaches_the_cli_entry_script_as_a_known_divergence() {
    let dir = make_test_dir("opcache_preload_global_scope");
    let preload = dir.join("preload.php");
    fs::write(&preload, "<?php $from_preload = 'SURVIVED';\n").unwrap();
    let bin = compile_source(
        &dir,
        "globalscope",
        "<?php\necho $from_preload ?? 'absent', \"\\n\";\n",
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", preload.display()),
        ],
    );

    assert_eq!(
        run_binary(&bin),
        "SURVIVED\n",
        "reference prints `absent` here; if elephc now does too, this divergence is CLOSED and \
         the test should be flipped to assert parity rather than removed"
    );
}

/// A conditional return exits the preloaded file and preserves nested function/finally behavior.
#[test]
fn a_conditional_preload_return_does_not_exit_the_entry_script() {
    let dir = make_test_dir("opcache_preload_conditional_return");
    fs::write(
        dir.join("lib.php"),
        r#"<?php
echo "PRELOAD\n";
function preload_nested_return_helper() {
    if (true) { return "FUNCTION"; }
    return "WRONG";
}
try {
    if (getenv("REVIEW_STOP") === "1") {
        for ($i = 0; $i < 1; $i++) { return; }
    }
    echo "TAIL\n";
} finally {
    echo "FINAL\n";
}
"#,
    )
    .unwrap();
    let source = r#"<?php echo preload_nested_return_helper(), "\n"; echo "MAIN\n";"#;
    let bin = compile_source(
        &dir,
        "conditional",
        source,
        &[
            "opcache.enable_cli=1".to_string(),
            "opcache.file_update_protection=0".to_string(),
            format!("opcache.preload={}", dir.join("lib.php").display()),
        ],
    );

    assert_eq!(
        run_binary_with_env(&bin, "REVIEW_STOP", "1"),
        "PRELOAD\nFINAL\nFUNCTION\nMAIN\n"
    );
    assert_eq!(
        run_binary_with_env(&bin, "REVIEW_STOP", "0"),
        "PRELOAD\nTAIL\nFINAL\nFUNCTION\nMAIN\n"
    );
}

/// A `return` NESTED in the preload file's control flow ends the PRELOAD, not the program.
///
/// Only a direct top-level `return` was confined to its file; one inside an `if` survived the
/// inlining and returned from the combined main function, so the entry script never ran. The
/// PR review's reproducer, verbatim, with the runtime condition both ways. MEASURED on reference
/// PHP 8.5: `PRELOAD MAIN` when the condition holds, `PRELOAD TAIL MAIN` when it does not.
#[test]
fn a_conditional_return_in_the_preload_ends_only_the_preload() {
    let dir = make_test_dir("opcache_preload_conditional_return");
    fs::write(
        dir.join("preload.php"),
        "<?php\n echo \"PRELOAD\\n\";\n if (getenv(\"REVIEW_STOP\") === \"1\") { return; }\n echo \"TAIL\\n\";\n",
    )
    .unwrap();
    let bin = compile_source(
        &dir,
        "main",
        "<?php echo \"MAIN\\n\";\n",
        &[
            "opcache.enable_cli=1".to_string(),
            "opcache.file_update_protection=0".to_string(),
            format!("opcache.preload={}", dir.join("preload.php").display()),
        ],
    );
    let run = |stop: &str| -> String {
        let output = Command::new(&bin)
            .env("REVIEW_STOP", stop)
            .output()
            .expect("failed to run compiled binary");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    assert_eq!(run("1"), "PRELOAD\nMAIN\n", "the return ends the preload only");
    assert_eq!(run("0"), "PRELOAD\nTAIL\nMAIN\n", "without it the preload runs to its end");
}

/// A declaration the preload graph makes under a guard survives preloading when the guard can be
/// decided without running anything the filter drops. symfony/polyfill-php86 ships
/// `SortDirection` as `if (\PHP_VERSION_ID < 80600) { enum SortDirection {...} }` in a stub the
/// preload file requires; the filter kept only bare declarations, so the enum vanished, and the
/// entry's own autoload copy then counted as a second declaration. A guard over a variable the
/// filter drops (Carbon's `require $method->hasReturnType() ? A : B`) is still left out.
#[test]
fn a_target_decided_guard_in_the_preload_graph_keeps_its_declaration() {
    let dir = make_test_dir("opcache_preload_guarded");
    fs::write(
        dir.join("stub.php"),
        "<?php\nif (\\PHP_VERSION_ID >= 80100) {\n    enum GuardedSort { case Ascending; case Descending; }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("runtime_guard.php"),
        "<?php\n$flag = time() > 0;\nif ($flag) {\n    class RuntimeGuarded {}\n}\n",
    )
    .unwrap();
    let preload = dir.join("preload.php");
    fs::write(
        &preload,
        "<?php\nrequire_once __DIR__ . '/stub.php';\nrequire_once __DIR__ . '/runtime_guard.php';\n",
    )
    .unwrap();

    let bin = compile_source(
        &dir,
        "user",
        r#"<?php
function order($d = GuardedSort::Ascending) { return $d === GuardedSort::Ascending ? "asc" : "desc"; }
$f = $argc > 5 ? 'nope' : 'order';
echo $f(), " ", order(GuardedSort::Descending), " ", var_export(enum_exists('GuardedSort', false), true), "\n";
"#,
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", preload.display()),
        ],
    );

    assert_eq!(run_binary(&bin), "asc desc true\n");
}

/// A class the preload SCRIPT ITSELF names in its `$classes[]` list is compiled, even when nothing
/// else reaches it but a computed `new $class`.
///
/// Symfony's generated `App_Kernel*Container.preload.php` is that script: its requires come first,
/// then `$classes[] = '…';` lines, among them the router's `RedirectableCompiledUrlMatcher`, which
/// `Router::getMatcher()` instantiates through `new $this->options['matcher_class']`. Only the
/// requires used to reach the scan, so a build fed that file as is left the class out; a
/// closed-world binary has no autoloader fallback and died with `Class "…" not found` on every
/// request. The build here is closed-world for the same reason: with the interpreter linked, the
/// runtime autoloader would hide the missing class.
#[test]
fn a_class_named_in_the_preload_script_itself_is_compiled() {
    // With a require before the list (Symfony's shape) and with nothing but the list.
    for with_require in [true, false] {
        assert_preload_script_class_list_is_compiled(with_require);
    }
}

/// One case of `a_class_named_in_the_preload_script_itself_is_compiled`.
fn assert_preload_script_class_list_is_compiled(with_require: bool) {
    let dir = make_test_dir(if with_require { "opcache_preload_script_class_list" } else { "opcache_preload_script_class_list_only" });
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"Lib\\Ext\\":"src/"}}}"#).unwrap();
    fs::write(
        dir.join("src/Matcher.php"),
        "<?php\nnamespace Lib\\Ext;\nfinal class Matcher {\n    public function value(): string { return 'matched'; }\n}\n",
    )
    .unwrap();
    // In its own directory, as Symfony's is (`var/cache/prod/`): every file beside the preload
    // script is compiled in anyway (`preload_directory_closure`), which would hide the list.
    // Without the require, nothing else is beside the script either: the list is then all the
    // script has, which is the case that used to return before the list was read.
    fs::create_dir_all(dir.join("var")).unwrap();
    if with_require {
        fs::write(dir.join("var/helpers.php"), "<?php\nfunction preload_helper(): string { return 'h'; }\n").unwrap();
    }
    let preload = dir.join("var/preload.php");
    fs::write(
        &preload,
        format!(
            "<?php\n{}$classes = [];\n$classes[] = 'Lib\\\\Ext\\\\Matcher';\n",
            if with_require { "require __DIR__ . '/helpers.php';\n" } else { "" }
        ),
    )
    .unwrap();
    let php = dir.join("user.php");
    fs::write(
        &php,
        "<?php\n$class = 'Lib\\\\Ext\\\\' . ($argc > 5 ? 'Other' : 'Matcher');\necho (new $class())->value(), \"\\n\";\n",
    )
    .unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .env("ELEPHC_CLOSED_WORLD", "1")
        .current_dir(&dir)
        .arg(&php)
        .arg("--ini")
        .arg("opcache.enable_cli=1")
        .arg("--ini")
        .arg(format!("opcache.preload={}", preload.display()))
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "closed-world compile failed (with_require={with_require}):\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(run_binary(&dir.join("user")), "matched\n");
}

/// A class file the preload names only through the `$classes[]` string list stays compiler-included
/// when a generated service factory reaches it through `include_once`.
///
/// Symfony's dumped container factories `include_once` the service class file. When that class is
/// named by the preload's `$classes[]` list (in a file the preload script requires, as Symfony's
/// preload does), the compiler takes its declarations from the preload graph; the file must then
/// be marked compiler-included so the interpreter skips the factory's `include_once`. Without the
/// mark the include executed the file through eval, and the class it declared shadowed the AOT
/// class in the interpreter: the dev container then reflected `HelloController::home` as
/// non-public and every route returned 500. The top-level marker here shows whether the runtime
/// include ran; the reflection line shows the class remains usable and public.
#[test]
fn a_class_file_named_by_the_preload_list_is_not_reincluded_at_runtime() {
    let dir = make_test_dir("opcache_preload_class_include_once");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"Lib\\Ext\\":"src/"}}}"#).unwrap();
    fs::write(
        dir.join("src/Ext.php"),
        "<?php\nnamespace Lib\\Ext;\necho 'X';\nfinal class Ext {\n    public function value(): string { return 'v'; }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("classes.php"),
        "<?php\n$classes = [];\n$classes[] = 'Lib\\Ext\\Ext';\n",
    )
    .unwrap();
    let preload = dir.join("preload.php");
    fs::write(&preload, "<?php\nrequire __DIR__ . '/classes.php';\n").unwrap();
    fs::write(
        dir.join("service.php"),
        "<?php\ninclude_once __DIR__ . '/src/Ext.php';\nfunction probe_ext(): string { $e = new \\Lib\\Ext\\Ext(); $m = new \\ReflectionMethod($e, 'value'); return ($m->isPublic() ? 'public' : 'hidden') . ':' . $e->value(); }\n",
    )
    .unwrap();
    let bin = compile_source(
        &dir,
        "user",
        "<?php\nrequire __DIR__ . '/service.php';\necho probe_ext(), \"\\n\";\n",
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", preload.display()),
        ],
    );
    assert_eq!(run_binary(&bin), "public:v\n");
}

/// A function declared after the class in a file the preload script names only as a class string
/// (Symfony's generated `$classes[] = '...';` list) is callable statically, by string callable and
/// from eval'd code. The resolver renames such a function to a private variant symbol and binds
/// the public name through a group and a mark; the preload filter kept the renamed declaration
/// and dropped both, which left a body nothing could reach (`twig_is_root_form()`).
#[test]
fn a_function_declared_in_a_class_file_named_by_the_preload_list_is_callable() {
    let dir = make_test_dir("opcache_preload_class_file_function");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"Lib\\Ext\\":"src/"}}}"#).unwrap();
    fs::write(
        dir.join("src/Ext.php"),
        "<?php\nnamespace Lib\\Ext;\nfinal class Ext {\n    public function tests(): array { return ['root' => 'Lib\\Ext\\is_root']; }\n}\nfunction is_root(int $x): bool { return $x === 0; }\n",
    )
    .unwrap();
    let lib = dir.join("lib.php");
    // The class list lives in a file the preload script REQUIRES, as in Symfony's generated one.
    fs::write(
        dir.join("classes.php"),
        "<?php\n$classes = [];\n$classes[] = 'Lib\\Ext\\Ext';\n",
    )
    .unwrap();
    fs::write(&lib, "<?php\nrequire __DIR__ . '/classes.php';\n").unwrap();
    let bin = compile_source(
        &dir,
        "user",
        r#"<?php
$t = (new Lib\Ext\Ext())->tests();
echo function_exists('Lib\Ext\is_root') ? "fn" : "nofn", "|";
echo \Lib\Ext\is_root(0) ? "yes" : "no", "|";
echo call_user_func($t['root'], 1) ? "yes" : "no", "|";
$code = 'return \Lib\Ext\is_root(0) ? "yes" : "no";';
echo eval($argc > 0 ? $code : 'return "";');
"#,
        &[
            "opcache.enable_cli=1".to_string(),
            format!("opcache.preload={}", lib.display()),
        ],
    );
    assert_eq!(run_binary(&bin), "fn|yes|no|yes");
}

/// A cache directory that still holds an earlier build's container stub compiles, and each stub
/// still loads its own container -- with that directory's preload alone, and as the FIRST of two
/// preloads (Symfony's console built to answer both `--env=dev` and `--env=prod`).
///
/// Symfony's `var/cache/<env>/` keeps the stub and the container class of every earlier dump next
/// to the live one (a debug and a non-debug kernel share `var/cache/dev/`). Each stub loads its
/// class behind `class_exists(…, false)`, and `preload_directory_closure` makes the stub an include
/// unit and joins every declarations-only file beside the preload. Two duplicates stopped the
/// build with "Duplicate class declaration": the stale class joined twice (required by the
/// closure, then included by its stub's unit), and the live one reached the preload's
/// declarations twice (the script's own require of the stub, then the stub's unit), which only a
/// LAST preload got away with -- its declarations are filtered against the entry's. Closed-world,
/// as a Symfony build is: the runtime include of a computed stub path is answered from the units.
#[test]
fn a_stale_container_stub_beside_the_preload_loads_its_class_once() {
    for with_second_preload in [false, true] {
        assert_stale_container_stub_compiles(with_second_preload);
    }
}

/// One case of `a_stale_container_stub_beside_the_preload_loads_its_class_once`.
fn assert_stale_container_stub_compiles(with_second_preload: bool) {
    let dir = make_test_dir(if with_second_preload {
        "opcache_preload_stale_container_stub_two_preloads"
    } else {
        "opcache_preload_stale_container_stub"
    });
    for (cache, name) in [("cache", "A"), ("cache", "B"), ("other", "P")] {
        fs::create_dir_all(dir.join(format!("{cache}/C{name}"))).unwrap();
        fs::write(
            dir.join(format!("{cache}/C{name}/K.php")),
            format!("<?php\nnamespace C{name};\nclass K {{ public function hi() {{ return '{name}'; }} }}\n"),
        )
        .unwrap();
        fs::write(
            dir.join(format!("{cache}/K{name}.php")),
            format!(
                "<?php\nif (\\class_exists(\\C{name}\\K::class, false)) {{\n    // no-op\n}} elseif (!include __DIR__.'/C{name}/K.php') {{\n    touch(__DIR__.'/C{name}.legacy');\n    return;\n}}\nreturn new \\C{name}\\K();\n"
            ),
        )
        .unwrap();
    }
    // Symfony's shape: the CLI returns first, then the live stub is required and used. `KB.php`
    // is the stale one.
    let preload = dir.join("cache/preload.php");
    fs::write(
        &preload,
        "<?php\nif (in_array(PHP_SAPI, ['cli'], true)) {\n    return;\n}\n(require __DIR__.'/KA.php')->hi();\n",
    )
    .unwrap();
    let second = dir.join("other/preload.php");
    fs::write(
        &second,
        "<?php\nif (in_array(PHP_SAPI, ['cli'], true)) {\n    return;\n}\n(require __DIR__.'/KP.php')->hi();\n",
    )
    .unwrap();
    let php = dir.join("user.php");
    fs::write(
        &php,
        "<?php\n$k = require __DIR__ . '/cache/K' . ($argc > 1 ? 'B' : 'A') . '.php';\necho $k->hi(), \"\\n\";\n",
    )
    .unwrap();
    let mut command = Command::new(elephc_bin());
    command
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .env("ELEPHC_CLOSED_WORLD", "1")
        .current_dir(&dir)
        .arg(&php)
        .arg("--ini")
        .arg("opcache.enable_cli=1")
        .arg("--ini")
        .arg(format!("opcache.preload={}", preload.display()));
    if with_second_preload {
        command.arg("--ini").arg(format!("opcache.preload={}", second.display()));
    }
    let output = command.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "closed-world compile failed (with_second_preload={with_second_preload}):\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let bin = dir.join("user");
    assert_eq!(run_binary(&bin), "A\n");
    let stale = Command::new(&bin).arg("stale").output().expect("failed to run binary");
    assert_eq!(String::from_utf8_lossy(&stale.stdout), "B\n");
}

/// A class file a preloaded class reaches only through an `include_once` in a METHOD answers the
/// runtime autoloader like php: the include succeeds and the interface then exists.
///
/// Symfony's dumped container writes `include_once` of a service class inside the service factory
/// method. The compiler takes that file's declarations, but nothing recorded where they came
/// from, so the file was never marked compiled: the autoload walk skipped the declared interface
/// and the closed-world runtime refused the autoloader's include of its file ("its top level
/// cannot be run from an include"). Recording it then answered the include with `1` but left
/// `interface_exists()` false: the file's activation cells were never raised.
#[test]
fn a_class_file_reached_only_from_a_preloaded_method_answers_the_autoloader() {
    let dir = make_test_dir("opcache_preload_method_include_once");
    fs::create_dir_all(dir.join("cache")).unwrap();
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::write(dir.join("lib/I.php"), "<?php\ninterface I { public function hi(): string; }\n").unwrap();
    fs::write(
        dir.join("cache/L.php"),
        "<?php\nclass L { public function f() { include_once __DIR__.'/../lib/I.php'; } }\n",
    )
    .unwrap();
    let preload = dir.join("cache/preload.php");
    fs::write(&preload, "<?php\nrequire __DIR__.'/L.php';\n").unwrap();
    let php = dir.join("user.php");
    fs::write(
        &php,
        "<?php\nspl_autoload_register(function ($c) { echo \"autoload $c\\n\"; include __DIR__.\"/lib/$c.php\"; });\n$n = $argc > 5 ? 'X' : 'I';\nvar_dump(interface_exists($n));\nclass K implements I { public function hi(): string { return 'hi'; } }\necho (new K())->hi(), \"\\n\";\n",
    )
    .unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .env("ELEPHC_CLOSED_WORLD", "1")
        .current_dir(&dir)
        .arg(&php)
        .arg("--ini")
        .arg("opcache.enable_cli=1")
        .arg("--ini")
        .arg(format!("opcache.preload={}", preload.display()))
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "closed-world compile failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(run_binary(&dir.join("user")), "autoload I\nbool(true)\nhi\n");
}
