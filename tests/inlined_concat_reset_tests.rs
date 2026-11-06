//! Purpose:
//! End-to-end tests that the EIR inliner cannot destroy a caller's in-flight concat
//! scratch — pinned as the invariant that `--ir-opt=on` and `--ir-opt=off` must print the
//! same bytes.
//!
//! THE BUG THESE PIN. `Op::ConcatReset` means "rewind the scratch buffer to MY frame's
//! base". Every function emits one at each statement boundary, and `capture_concat_base`
//! gives each frame its own base, so a real call can never reach the caller's scratch.
//! Splicing a body into its caller merges the frames, and that sentence silently starts
//! pointing at the HOST's base — so the callee's statement boundary frees scratch the
//! caller is still holding:
//!
//! ```text
//! printf("B:%s|%s\n", "'$v'", var_export(plain(), true));
//! reference: B:'garbage'|'2'
//! elephc:    B::garbage'|'2'      // the temporary's first byte, overwritten
//! ```
//!
//! WHY THE EXISTING GUARD DID NOT CATCH IT. `call_string_args_are_stable` covered exactly
//! this hazard for the callee's own `Str` ARGUMENTS. `plain()` takes none, so it passed
//! vacuously — and the value actually destroyed was a SIBLING argument of the enclosing
//! `printf`, which is not an argument to the inlined function at all. A guard scoped to the
//! callee's parameters cannot see the caller's other live temporaries.
//!
//! WHY `--ir-opt` IS THE ASSERTION AND NOT A LITERAL ALONE. The literal pins today's
//! reference answer; the cross-check pins the property that makes it a codegen bug rather
//! than a formatting one. An optimization pass that changes observable output is wrong even
//! when the new output happens to look plausible, and only the two-build comparison says so.
//! Both are asserted: a future change that corrupts BOTH builds identically would satisfy
//! the comparison alone.
//!
//! Called from:
//! - `cargo test --test inlined_concat_reset_tests` through Rust's test harness.
//!
//! Key details:
//! - The callee must be small enough to be selected for inlining; `plain()` returning a
//!   literal is about as small as a function gets. If the inliner's selection is ever
//!   narrowed past it these tests keep passing while pinning nothing, which is why
//!   `the_optimizer_actually_inlines_the_callee` checks the emitted assembly directly.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir. Host-target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

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

/// Compiles `source` under the given extra flags and returns the program's stdout.
fn compile_and_run(dir: &Path, stem: &str, source: &str, flags: &[&str]) -> String {
    let php = dir.join(format!("{stem}.php"));
    fs::write(&php, source).unwrap();

    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    for flag in flags {
        cmd.arg(flag);
    }
    let compiled = cmd.output().expect("failed to spawn elephc");
    assert!(
        compiled.status.success(),
        "compilation failed ({flags:?}):\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr),
    );

    let output = Command::new(dir.join(stem))
        .output()
        .expect("failed to run binary");
    assert!(
        output.status.success(),
        "binary failed ({flags:?}):\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Returns the assembly from the program's entry label onward.
///
/// THE LEADING UNDERSCORE IS MACH-O's, NOT THE COMPILER'S. Darwin decorates every symbol
/// with `_`, so the entry is `_main:` there and plain `main:` on Linux — on both the
/// `ubuntu-24.04` and `ubuntu-24.04-arm` runners CI uses. Matching one spelling made these
/// tests pass locally and panic on two of the three CI platforms, AFTER a successful
/// compile, which is the least informative way to fail.
fn entry_onward(asm: &str) -> &str {
    for label in ["\n_main:", "\nmain:"] {
        if let Some(at) = asm.find(label) {
            return &asm[at..];
        }
    }
    panic!("no entry label in the assembly");
}

/// Returns whether `body` CALLS `symbol`, on either architecture and either platform.
///
/// Four spellings: `bl` on AArch64 and `call` on x86_64, each with or without Mach-O's
/// leading underscore. Searching for the bare symbol name would not do — the callee's own
/// label is `<symbol>:`, and it can sit inside the slice being searched, so a plain
/// `contains` reports a call that is really a definition.
fn body_calls(body: &str, symbol: &str) -> bool {
    [
        format!("bl {symbol}"),
        format!("bl _{symbol}"),
        format!("call {symbol}"),
        format!("call _{symbol}"),
    ]
    .iter()
    .any(|spelling| body.contains(spelling.as_str()))
}

/// Runs `source` with the optimizer on and off, asserts both printed `expected`, and
/// returns the optimized output.
fn assert_same_with_and_without_opt(prefix: &str, source: &str, expected: &str) -> String {
    let dir = make_test_dir(prefix);
    let optimized = compile_and_run(&dir, "opt_on", source, &[]);
    let plain = compile_and_run(&dir, "opt_off", source, &["--ir-opt=off"]);
    assert_eq!(
        optimized, plain,
        "the optimizer changed the program's output; unoptimized is the reference reading"
    );
    assert_eq!(optimized, expected, "output does not match reference PHP");
    optimized
}

/// The reported miscompile, reduced: a zero-parameter callee spliced into a call whose
/// EARLIER argument is a live interpolated temporary.
///
/// MEASURED against reference PHP 8.5: `B:'garbage'|'2'`. Before the fix elephc printed
/// `B::garbage'|'2'` with `--ir-opt=on` and the correct bytes with `--ir-opt=off`.
#[test]
fn an_inlined_callee_does_not_free_a_sibling_arguments_scratch() {
    assert_same_with_and_without_opt(
        "inline_reset_sibling",
        r#"<?php
function plain(): string { return '2'; }
$v = 'garbage';
printf("B:%s|%s\n", "'$v'", var_export(plain(), true));
"#,
        "B:'garbage'|'2'\n",
    );
}

/// The same hazard reached through an injected prelude rather than a user function.
///
/// `ini_get()` is compiler-generated PHP, and its statements only began emitting concat
/// resets when the reset stopped being skipped for non-source spans — so this spelling is
/// the one that regressed, while the user-function spelling above was already wrong.
///
/// THE LITERAL PREFIX IN THE FORMAT IS LOAD-BEARING. `"a:%s|%s\n"` corrupts and
/// `"%s|%s\n"` does not: the prefix shifts where the output is assembled relative to the
/// freed temporary, so without it the overwrite lands harmlessly and the test passes
/// against the broken compiler. VERIFIED by mutation — the prefixless spelling survived
/// the reverted fix, this one does not.
///
/// MEASURED against reference: `a:'garbage'|'2'`.
#[test]
fn an_inlined_prelude_function_does_not_free_the_callers_scratch() {
    assert_same_with_and_without_opt(
        "inline_reset_prelude",
        r#"<?php
$v = 'garbage';
printf("a:%s|%s\n", "'$v'", var_export(ini_get('opcache.revalidate_freq'), true));
"#,
        "a:'garbage'|'2'\n",
    );
}

/// A longer argument list, so a single off-by-one cannot satisfy the assertion by accident:
/// three live interpolated temporaries straddling two spliced bodies.
///
/// MEASURED against reference: `<a>|<b>|<c>|'1'|'2'`.
#[test]
fn several_live_temporaries_survive_two_splices() {
    assert_same_with_and_without_opt(
        "inline_reset_many",
        r#"<?php
function one(): string { return '1'; }
function two(): string { return '2'; }
$a = 'a'; $b = 'b'; $c = 'c';
printf("%s|%s|%s|%s|%s\n", "<$a>", "<$b>", "<$c>",
       var_export(one(), true), var_export(two(), true));
"#,
        "<a>|<b>|<c>|'1'|'2'\n",
    );
}

/// Verifies a callee that LOOPS is not inlined, and that such a program still finishes.
///
/// This test used to claim the opposite and could not have shown it. Two reviewers argued
/// that neutralizing a transplanted `concat_reset` lets a spliced loop run away, and they
/// were right about the consequence even though the mechanism they proposed — an unbounded
/// write past `_concat_buf` — is not what happens. `__rt_concat_reserve` IS bounded: at
/// 64 KiB it stops handing out scratch and starts returning OWNED HEAP BLOCKS. Those are
/// what accumulate, for as long as the host statement lasts, and a spliced loop makes that
/// the whole loop.
///
/// MEASURED before the gate, on the carrier below at two million iterations: `--ir-opt=on`
/// died with `Fatal error: heap memory exhausted` while `--ir-opt=off` completed and printed
/// 14,888,902 bytes. An optimization that decides whether a program finishes is not one.
///
/// THE EARLIER VERSION OF THIS TEST WAS VACUOUS, which is worth recording because it looked
/// exactly like a strong test. It called the callee with `""`, so `$a . $a` was a zero-length
/// concat: `reserve(0)` always fits and `publish` advances by zero, and `_concat_off` never
/// moved at all. Its docblock claimed "80 KB of concat through a 64 KiB arena" and described
/// a run that was never committed — the probe used `"z"`, the test shipped `""`.
///
/// Both halves are asserted now. The behavioural half runs the carrier at the scale that
/// used to fail; the structural half reads the assembly, because once the gate is in place
/// the behavioural half passes for a reason that has nothing to do with this pass and would
/// keep passing if the gate were deleted and the runaway returned.
#[test]
fn a_callee_that_loops_is_not_inlined() {
    const CARRIER: &str = r#"<?php
function churn(int $n): int { for ($i = 0; $i < $n; $i++) { echo "y" . $i; } return $n; }
echo churn(2000000), "\nEND\n";
"#;

    let dir = make_test_dir("inline_reset_loop_gate");
    let optimized = compile_and_run(&dir, "opt_on", CARRIER, &[]);
    let plain = compile_and_run(&dir, "opt_off", CARRIER, &["--ir-opt=off"]);
    assert_eq!(
        optimized.len(),
        plain.len(),
        "the optimizer changed how much the program managed to print"
    );
    assert!(
        optimized.ends_with("END\n"),
        "the optimized build did not reach the end of the program"
    );

    // The structural half: this callee must NOT have been spliced.
    let php = dir.join("asm.php");
    fs::write(&php, CARRIER).unwrap();
    let out = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg(&php)
        .arg("--emit-asm")
        .output()
        .expect("failed to spawn elephc");
    assert!(out.status.success(), "asm emission failed");
    let asm = fs::read_to_string(dir.join("asm.s")).expect("no asm.s emitted");
    assert!(
        !entry_onward(&asm).contains("inline_cont"),
        "a looping callee was spliced into the entry; the eligibility gate is gone and \
         the heap-exhaustion runaway is back"
    );
}

/// Guards the premise of the tests above: the optimizer must actually splice `plain()` in,
/// or they compare two identical unoptimized builds and pin nothing.
///
/// The check is on the emitted assembly rather than on a timing or a symbol count: an
/// inlined body leaves its continuation block behind, and the call to the callee's own
/// label disappears from the caller.
#[test]
fn the_optimizer_actually_inlines_the_callee() {
    let dir = make_test_dir("inline_reset_premise");
    let php = dir.join("main.php");
    fs::write(
        &php,
        r#"<?php
function plain(): string { return '2'; }
$v = 'garbage';
printf("B:%s|%s\n", "'$v'", var_export(plain(), true));
"#,
    )
    .unwrap();

    let out = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg(&php)
        .arg("--emit-asm")
        .output()
        .expect("failed to spawn elephc");
    assert!(
        out.status.success(),
        "asm emission failed:\nstderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("no main.s emitted");
    let body = entry_onward(&asm);
    assert!(
        body.contains("inline_cont"),
        "the optimizer did not inline anything into the entry, so the sibling-scratch \
         tests are comparing two unoptimized builds"
    );
    assert!(
        !body_calls(body, "fn__u_plain"),
        "the entry still calls plain() rather than inlining it"
    );
}
