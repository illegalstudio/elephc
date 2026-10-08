//! Purpose:
//! End-to-end tests for PHP resource NUMBERING across the `eval()` boundary.
//!
//! PHP keeps ONE resource-id counter per request, and code running inside `eval()`
//! draws from it exactly like the code around it. Measured against PHP 8.5.6:
//!
//! ```text
//! $h = fopen(...);  eval('$a = fopen(...); $b = fopen(...);');  $i = fopen(...);
//!   ->  5                     6              7                      8
//! ```
//!
//! elephc runs `eval()` through the `elephc-magician` interpreter, which owns its
//! resources in `EvalStreamResources` — a dense zero-based table entirely unrelated
//! to the file descriptors and allocator handles the compiled program produces. Both
//! sides nevertheless box their resources through `__rt_mixed_from_value`, whose
//! tag-9 arm binds a PHP id in the runtime registry
//! (`src/codegen_support/runtime/resource_ids.rs`), so both sides were keying ONE
//! table with two incompatible numbering schemes. Two defects followed:
//!
//! - Eval's first three keys were 0, 1 and 2, which the registry answers directly as
//!   `payload + 1` because those are the standard stream descriptors. So the first
//!   three eval resources reported 1, 2, 3 and the fourth jumped to the counter's 5:
//!   `get_resource_id()` inside `eval()` returned 1,2,3,5,6,7 where PHP returns
//!   5,6,7,8,9,10. Those three also consumed nothing from the shared counter, so
//!   every host resource created after an `eval()` reported an id too low.
//! - Past the third, eval key `n` and host descriptor `n` are the same registry key.
//!   An eval stream would find the binding of an unrelated host stream and report ITS
//!   id — two live resources, one number.
//!
//! The fix gives eval payloads their own key namespace
//! (`elephc_magician::stream_resources::EVAL_RESOURCE_PAYLOAD_BASE`) while leaving
//! the ID space shared, which is what PHP does.
//!
//! Called from:
//! - `cargo test --test eval_resource_id_tests` through Rust's test harness.
//!
//! Key details:
//! - EVERY expected string here was captured from reference PHP 8.5.6 with
//!   `php -d xdebug.mode=off` — mandatory, because the host `php` loads Xdebug, which
//!   overloads `var_dump`. The exact program each capture came from is the `source`
//!   literal in the test that asserts it.
//! - Harness style mirrors `tests/resource_id_and_hash_context_tests.rs`: the elephc
//!   CLI (`CARGO_BIN_EXE_elephc`) runs as a subprocess in an isolated temp dir,
//!   compiles to a plain executable, runs it, and its stdout is asserted. Host-target
//!   only.
//! - THE COVERAGE IS DELIBERATELY THREE-SIDED: eval-only, host-only, and mixed. The
//!   host-only test is not redundant with `resource_id_and_hash_context_tests.rs`; it
//!   pins that the eval namespace base did not leak into host numbering, which is the
//!   way a future change to the base would most plausibly go wrong.
//! - Programs use `php://memory` rather than files wherever the identity of the
//!   underlying stream does not matter, so the ids do not depend on which descriptor
//!   numbers the host happens to have free. The one test that DOES need real
//!   descriptors (`eval_streams_never_alias_a_host_descriptor`) opens fixture files
//!   written from Rust, because PHP's `file_put_contents()` consumes a resource id and
//!   elephc's does not — creating fixtures from PHP would put an unrelated divergence
//!   inside an id assertion.

#[path = "support/managed_pcre2.rs"]
mod managed_pcre2_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Expected capability reminder for eval fixtures that intentionally omit regex support.
const EVAL_WITHOUT_REGEX_REMINDER: &str =
    "warning: dynamic eval was compiled without optional regex support";

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
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

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather
/// than anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo`
/// glibc notes and the `.note.GNU-stack` deprecation, while Apple's linker stays silent.
/// The intentional eval capability reminder is excluded exactly; every other compiler
/// diagnostic still surfaces.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            *line != EVAL_WITHOUT_REGEX_REMINDER
                && (line.starts_with("error")
                    || line.starts_with("Error")
                    || line.starts_with("warning")
                    || line.starts_with("Warning: ")
                    || line.starts_with("EIR backend error"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source` to a plain executable, asserting elephc reported no diagnostic.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    managed_pcre2_support::configure_host_managed_pcre2(&mut cmd, dir);
    cmd.current_dir(dir);
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    let diagnostics = elephc_diagnostics(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "elephc compile failed:\n{diagnostics}"
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostic:\n{diagnostics}"
    );
    dir.join(stem)
}

/// Runs a compiled executable IN ITS OWN TEMP DIR and returns its stdout.
///
/// Setting the working directory is not cosmetic: `Command::new(bin)` would otherwise
/// inherit the harness's cwd (the repository root), so a program opening `"first.txt"`
/// would read a file next to `Cargo.toml`.
fn run_binary_in(dir: &Path, bin: &Path) -> String {
    let output = Command::new(bin)
        .current_dir(dir)
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

/// Writes the fixture files the descriptor-aliasing program opens.
///
/// Written from RUST, not from PHP: `file_put_contents()` consumes a resource id under
/// PHP and does not under elephc, so creating fixtures from the program under test would
/// fold an unrelated divergence into every id assertion.
fn write_fixture_files(dir: &Path) {
    fs::write(dir.join("first.txt"), b"a\n").unwrap();
    fs::write(dir.join("second.txt"), b"b\n").unwrap();
}

/// Compiles `source`, runs it, and asserts stdout equals `expected`.
fn assert_program_output(prefix: &str, source: &str, expected: &str) {
    let dir = make_test_dir(prefix);
    write_fixture_files(&dir);
    let bin = compile(&dir, source, prefix);
    assert_eq!(run_binary_in(&dir, &bin), expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies resources created INSIDE `eval()` are numbered 5, 6, 7, ... like PHP's.
///
/// This is the regression the file exists for. Before the eval payload namespace, this
/// exact program printed `1,2,3,5,6,7` and `resource(0) of type (stream)`.
#[test]
fn eval_resources_are_numbered_consecutively_from_five() {
    let source = r#"<?php
eval('
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
$c = fopen("php://memory", "r");
$d = fopen("php://memory", "r");
$e = fopen("php://memory", "r");
$f = fopen("php://memory", "r");
echo get_resource_id($a), ",", get_resource_id($b), ",", get_resource_id($c), ",";
echo get_resource_id($d), ",", get_resource_id($e), ",", get_resource_id($f), "\n";
var_dump($a);
');
"#;
    assert_program_output(
        "elephc_eval_rid_eval_only",
        source,
        "5,6,7,8,9,10\nresource(5) of type (stream)\n",
    );
}

/// Verifies the SAME program without `eval()` keeps the numbering it always had.
///
/// Guards the direction a change to the eval namespace base would most plausibly break:
/// host resources are keyed by descriptor number and must stay untouched by anything
/// done for eval.
#[test]
fn host_resources_are_numbered_consecutively_from_five() {
    let source = r#"<?php
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
$c = fopen("php://memory", "r");
$d = fopen("php://memory", "r");
$e = fopen("php://memory", "r");
$f = fopen("php://memory", "r");
echo get_resource_id($a), ",", get_resource_id($b), ",", get_resource_id($c), ",";
echo get_resource_id($d), ",", get_resource_id($e), ",", get_resource_id($f), "\n";
var_dump($a);
"#;
    assert_program_output(
        "elephc_eval_rid_host_only",
        source,
        "5,6,7,8,9,10\nresource(5) of type (stream)\n",
    );
}

/// Verifies host and eval resources share ONE counter and interleave in creation order.
///
/// The mixed shape is what proves the id space is shared rather than merely
/// self-consistent on each side: a design that gave eval its own counter would print
/// `5 / 5,6 / 6` here and pass both single-sided tests above.
#[test]
fn host_and_eval_resources_draw_from_one_shared_counter() {
    let source = r#"<?php
$h1 = fopen("php://memory", "r");
echo "h1=", get_resource_id($h1), "\n";
eval('
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
echo "e=", get_resource_id($a), ",", get_resource_id($b), "\n";
');
$h2 = fopen("php://memory", "r");
echo "h2=", get_resource_id($h2), "\n";
"#;
    assert_program_output(
        "elephc_eval_rid_mixed",
        source,
        "h1=5\ne=6,7\nh2=8\n",
    );
}

/// Verifies an eval stream never inherits the id bound to a live host DESCRIPTOR.
///
/// The fourth eval stream is the one that matters: its eval key used to be 3, the same
/// registry key as the host's first real file descriptor, so it looked that host stream
/// up and reported ITS id. `h1again` re-reads the host handle afterwards to show the
/// binding was not merely overwritten in the other direction — every id in this program
/// is distinct and creation-ordered.
#[test]
fn eval_streams_never_alias_a_host_descriptor() {
    let source = r#"<?php
$h1 = fopen("first.txt", "r");
echo "h1=", get_resource_id($h1), "\n";
eval('
$e1 = fopen("php://memory", "r");
$e2 = fopen("php://memory", "r");
$e3 = fopen("php://memory", "r");
$e4 = fopen("php://memory", "r");
echo "e=", get_resource_id($e1), ",", get_resource_id($e2), ",", get_resource_id($e3), ",", get_resource_id($e4), "\n";
');
$h2 = fopen("second.txt", "r");
echo "h2=", get_resource_id($h2), "\n";
echo "h1again=", get_resource_id($h1), "\n";
"#;
    assert_program_output(
        "elephc_eval_rid_alias",
        source,
        "h1=5\ne=6,7,8,9\nh2=10\nh1again=5\n",
    );
}

/// Verifies a closed eval stream's id is burned, not recycled, as in php-src.
///
/// `zend_resource` handles come from a monotonically increasing list index, so the
/// stream opened after an `fclose()` gets the NEXT id and the closed one's is never
/// seen again — PHP 8.5.6 prints `5,7` for this program, not `5,6`.
#[test]
fn a_closed_eval_stream_does_not_release_its_id() {
    let source = r#"<?php
eval('
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
fclose($b);
$c = fopen("php://memory", "r");
echo get_resource_id($a), ",", get_resource_id($c), "\n";
');
"#;
    assert_program_output("elephc_eval_rid_reopen", source, "5,7\n");
}

/// Verifies eval resource ids are STABLE across two runs of the same binary.
///
/// This is the shape that separates "deterministic" from "happens to look right". The
/// defect that created the id registry was an id derived from a malloc'd address, which
/// is identical within a run and differs between runs under ASLR, so only a cross-run
/// comparison can see it. Eval payloads are allocator-independent by construction, and
/// this test is what keeps them that way.
#[test]
fn eval_resource_ids_are_stable_across_runs_of_one_binary() {
    let source = r#"<?php
eval('
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
$d = opendir(".");
$c = fopen("php://memory", "r");
echo get_resource_id($a), ",", get_resource_id($b), ",", get_resource_id($c), "\n";
');
"#;
    let dir = make_test_dir("elephc_eval_rid_stable");
    write_fixture_files(&dir);
    let bin = compile(&dir, source, "elephc_eval_rid_stable");
    let first = run_binary_in(&dir, &bin);
    let second = run_binary_in(&dir, &bin);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(first, "5,6,8\n", "first run diverged from PHP 8.5.6");
    assert_eq!(second, first, "eval resource ids changed between two runs");
}

/// Verifies every string context inside an AOT-compiled `eval()` renders a HOST
/// resource the way PHP 8.5.6 does, open and closed.
///
/// Captured from PHP 8.5.6 (`php -d xdebug.mode=off`) running this exact program.
/// A literal `eval()` body that elephc can compile ahead of time becomes an EIR scope
/// function, so it reaches the very same `__rt_mixed_cast_string` the host program
/// uses — which is why all four value-producing forms were empty here too, and why one
/// tag-9 arm fixes both. The two trailing lines pin the closed handle: PHP keeps
/// printing `Resource id #5` after `fclose()`.
#[test]
fn eval_renders_a_host_resource_in_every_string_context() {
    let source = r#"<?php
$r = fopen("first.txt", "r");
eval('echo "interp:$r\n";');
eval('echo "concat:" . $r . "\n";');
eval('echo "cast:" . (string) $r . "\n";');
eval('echo "strval:" . strval($r) . "\n";');
eval('echo "print:"; print $r; echo "\n";');
fclose($r);
eval('echo "closed-interp:$r\n";');
eval('echo "closed-strval:" . strval($r) . "\n";');
"#;
    assert_program_output(
        "elephc_eval_res_str_aot",
        source,
        "interp:Resource id #5\nconcat:Resource id #5\ncast:Resource id #5\n\
         strval:Resource id #5\nprint:Resource id #5\n\
         closed-interp:Resource id #5\nclosed-strval:Resource id #5\n",
    );
}

/// Verifies the same rendering through the RUNTIME eval interpreter, which is a
/// different code path from the AOT one above.
///
/// Captured from PHP 8.5.6 running this exact program. The `eval()` argument is built
/// at run time (`explode()` over a joined literal), so elephc cannot compile the body
/// ahead of time and `elephc-magician` interprets it instead. `strval()` is the form
/// that separates the two paths: the interpreter reaches it through
/// `RuntimeValueOps::cast_string`, whose host wrapper `__elephc_eval_value_cast_string`
/// re-implements its OWN tag dispatch in the eval bridge. That dispatch needed its own
/// tag-9 arm; with only the boxed-Mixed cast fixed, `strval($r)` still returned the
/// empty string here while concatenation already worked.
#[test]
fn the_eval_interpreter_renders_a_host_resource_in_every_string_context() {
    let source = r#"<?php
$r = fopen("first.txt", "r");
$srcs = explode("@@", 'echo "concat:" . $r . "\n";@@echo "cast:" . (string) $r . "\n";@@echo "strval:" . strval($r) . "\n";@@echo "print:"; print $r; echo "\n";');
foreach ($srcs as $s) {
    eval($s);
}
fclose($r);
$after = explode("@@", 'echo "closed-concat:" . $r . "\n";@@echo "closed-strval:" . strval($r) . "\n";');
foreach ($after as $s) {
    eval($s);
}
"#;
    assert_program_output(
        "elephc_eval_res_str_dyn",
        source,
        "concat:Resource id #5\ncast:Resource id #5\nstrval:Resource id #5\n\
         print:Resource id #5\nclosed-concat:Resource id #5\nclosed-strval:Resource id #5\n",
    );
}

/// Verifies a resource CREATED inside `eval()` renders like PHP's too.
///
/// Captured from PHP 8.5.6 running this exact program. An eval-created stream is keyed
/// in `EvalStreamResources` and lifted clear of host descriptor numbers by
/// `EVAL_RESOURCE_PAYLOAD_BASE`, so this exercises the registry through a payload no
/// host resource can present while still sharing the one id counter — `Resource id #5`
/// is the first id in the request either way.
#[test]
fn an_eval_created_resource_renders_in_every_string_context() {
    let source = r#"<?php
eval('
$r = fopen("first.txt", "r");
echo "concat:" . $r . "\n";
echo "cast:" . (string) $r . "\n";
echo "strval:" . strval($r) . "\n";
echo "print:"; print $r; echo "\n";
');
"#;
    assert_program_output(
        "elephc_eval_res_str_inner",
        source,
        "concat:Resource id #5\ncast:Resource id #5\nstrval:Resource id #5\n\
         print:Resource id #5\n",
    );
}

/// Verifies `hash_init()` inside `eval()` consumes NO PHP resource id.
///
/// PHP 8's `hash_init()` returns a `HashContext` OBJECT. Objects and resources are two
/// unrelated numbering spaces in php-src (`zend_object.handle` vs `zend_resource.handle`),
/// so a hash context takes nothing from the counter `get_resource_id()` reports. elephc's
/// NATIVE side already modelled that — `__rt_hash_init` boxes resource kind 2, which
/// `__rt_mixed_from_value` excludes from id binding — but the eval interpreter boxed its
/// context as an ordinary resource, so this exact program printed `6,7` where PHP prints
/// `5,6`. The fix gives eval's context its own excluded kind (5).
///
/// TWO streams, not one: a single id would still look plausible if the counter were
/// merely offset. The pair pins that the counter is intact, not just shifted back.
#[test]
fn eval_hash_init_consumes_no_resource_id() {
    let source = r#"<?php
eval('
$h = hash_init("md5");
$x = fopen("php://memory", "r");
$y = fopen("php://memory", "r");
echo get_resource_id($x), ",", get_resource_id($y), "\n";
');
"#;
    assert_program_output("elephc_eval_hash_no_id", source, "5,6\n");
}

/// Verifies an `eval()` that hashes does not shift the HOST program's resource ids.
///
/// The strongest shape of the same defect, and the reason it mattered beyond `eval()`:
/// PHP keeps ONE resource-id counter per request and elephc deliberately shares its
/// registry across the eval boundary, so an id burned inside `eval()` moved every
/// resource the compiled program created AFTERWARDS. Before the fix this printed
/// `5 / <digest> / 7`; PHP 8.5.6 prints `5 / <digest> / 6`.
///
/// The digest is asserted too, so the test cannot pass by breaking hashing: an
/// implementation that made `hash_init()` fail outright would also consume no id.
#[test]
fn eval_hashing_does_not_shift_host_resource_ids() {
    let source = r#"<?php
$f0 = fopen("php://memory", "r");
echo get_resource_id($f0), "\n";
eval('
$h = hash_init("md5");
hash_update($h, "abc");
echo hash_final($h), "\n";
');
$f1 = fopen("php://memory", "r");
echo get_resource_id($f1), "\n";
"#;
    assert_program_output(
        "elephc_eval_hash_host_ids",
        source,
        "5\n900150983cd24fb0d6963f7d28e17f72\n6\n",
    );
}

/// Verifies `hash_copy()` and repeated `hash_init()` are id-free too, and still hash.
///
/// `hash_copy()` leaked an id by the same mechanism, and each `hash_init()` leaked one
/// of its own: before the fix this program printed `8` for the inner stream and `9` for
/// the outer one, against PHP's `5` and `6`. The two digests pin that the contexts stay
/// independent across the copy — `$a` sees only "abc", `$b` sees "abcdef" — so the ids
/// cannot be made to match by degrading the contexts into a single shared one.
#[test]
fn eval_hash_copy_and_repeated_hash_init_consume_no_resource_ids() {
    let source = r#"<?php
eval('
$a = hash_init("md5");
hash_update($a, "abc");
$b = hash_copy($a);
hash_update($b, "def");
$c = hash_init("sha1");
$x = fopen("php://memory", "r");
echo get_resource_id($x), "\n";
echo hash_final($a), "\n";
echo hash_final($b), "\n";
');
$y = fopen("php://memory", "r");
echo get_resource_id($y), "\n";
"#;
    assert_program_output(
        "elephc_eval_hash_copy_no_id",
        source,
        "5\n900150983cd24fb0d6963f7d28e17f72\ne80b5017098950fc58aad83c8c14978e\n6\n",
    );
}

/// Verifies a resource CREATED and CLOSED inside `eval()` renames its type to `Unknown`.
///
/// Captured from PHP 8.5.6 running this exact program. An eval-created resource carries no
/// close sentinel — its payload IS the key of `EvalStreamResources`, and negating it the
/// way the compiled side does would break every builtin that later resolves the handle —
/// so the interpreter reads the close state from the tables instead
/// (`EvalStreamResources::is_live`). Both display sites are asserted, because
/// `get_resource_type()` was a second, independent constant `"stream"`.
///
/// The last two lines are the guard for the numbering this must not disturb: the closed
/// handle keeps id 5, and the next `fopen()` inside the same `eval()` still gets 6.
#[test]
fn an_eval_created_resource_reports_the_type_unknown_once_closed() {
    let source = r#"<?php
eval('
$r = fopen("first.txt", "r");
var_dump($r);
var_dump(get_resource_type($r));
fclose($r);
var_dump($r);
var_dump(get_resource_type($r));
var_dump(get_resource_id($r));
$n = fopen("first.txt", "r");
var_dump($n);
');
"#;
    assert_program_output(
        "elephc_eval_res_type_own",
        source,
        "resource(5) of type (stream)\nstring(6) \"stream\"\n\
         resource(5) of type (Unknown)\nstring(7) \"Unknown\"\nint(5)\n\
         resource(6) of type (stream)\n",
    );
}

/// Verifies a HOST resource closed by the compiled program reports `Unknown` when it is
/// displayed from a RUNTIME-INTERPRETED `eval()`.
///
/// Captured from PHP 8.5.6 running this exact program. This is the OTHER close
/// representation and a different code path from the test above: the cell arrives inside
/// `eval()` carrying the NEGATIVE `-id` sentinel that `fclose` stamped into its Mixed box,
/// with no `EvalStreamResources` entry to consult. Reading that payload through
/// `i64::try_from` — the way `eval_resource_payload()` does — rejects it outright, so the
/// predicate reads it as `as i64`.
///
/// The `eval()` argument is built at run time (`explode()` over a joined literal) so
/// elephc cannot compile the body ahead of time and `elephc-magician` interprets it; a
/// literal `eval('var_dump($r);')` is AOT-compiled and would only re-test the native fix.
#[test]
fn the_eval_interpreter_reports_a_closed_host_resource_as_unknown() {
    let source = r#"<?php
$r = fopen("first.txt", "r");
$probe = explode("@@", 'var_dump($r);@@var_dump(get_resource_type($r));');
foreach ($probe as $s) {
    eval($s);
}
fclose($r);
foreach ($probe as $s) {
    eval($s);
}
$after = explode("@@", 'var_dump(get_resource_id($r));');
foreach ($after as $s) {
    eval($s);
}
$n = fopen("first.txt", "r");
var_dump($n);
"#;
    assert_program_output(
        "elephc_eval_res_type_host",
        source,
        "resource(5) of type (stream)\nstring(6) \"stream\"\n\
         resource(5) of type (Unknown)\nstring(7) \"Unknown\"\nint(5)\n\
         resource(6) of type (stream)\n",
    );
}
