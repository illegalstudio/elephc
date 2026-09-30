//! Purpose:
//! Regression coverage for feature-gated runtime and synthetic builtin reachability.
//!
//! Called from:
//! - `cargo test` through the codegen integration-test harness.
//!
//! Key details:
//! - Plain native programs must not carry the optional eval Reflection surface.
//! - A read whose path is a constant local string must not carry the URL reader.

use crate::support::{compile_and_run, compile_source_to_asm_with_options, fs, make_cli_test_dir};

/// Verifies a program without eval or Reflection omits their synthetic methods and metadata.
#[test]
fn test_plain_program_omits_unreferenced_reflection_surface() {
    let dir = make_cli_test_dir("elephc_plain_runtime_reachability");
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php echo 1;", &dir, 8_388_608, false, false);

    assert!(
        !user_asm.contains("@fn name=Reflection"),
        "plain program unexpectedly lowered synthetic Reflection methods"
    );
    assert!(
        !user_asm.contains("_eval_reflection_"),
        "plain program unexpectedly emitted eval Reflection metadata"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|library| library == "elephc_magician"),
        "plain program unexpectedly requested the Magician bridge"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a constant local path does not drag the URL reader into the program.
///
/// PHP's wrapper grammar requires `scheme://`, so a literal without the separator cannot name a
/// wrapper and the URL multiplexer's tests provably cannot succeed. Entering at the phar level
/// instead takes the URL reader out of the call graph, and with it `socket`/`connect`/`bind` and
/// the resolver — measured as 11 distinct syscalls down to 7 on `file_get_contents("/etc/hosts")`.
///
/// The regression this guards is silent: routing the literal back through the multiplexer keeps
/// every behaviour test passing while quietly restoring the whole network stack to any program
/// that reads a constant path.
#[test]
fn test_constant_local_path_omits_the_url_reader() {
    let dir = make_cli_test_dir("elephc_constant_path_reachability");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        "<?php echo file_get_contents(\"/etc/hosts\");",
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        user_asm.contains("__rt_file_get_contents_maybe_phar"),
        "a constant local path should enter the read at the phar level"
    );
    assert!(
        !user_asm.contains("__rt_file_get_contents_maybe_url"),
        "a constant local path must not reach the URL multiplexer"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a path the compiler cannot see through still reaches the URL reader.
///
/// The specialisation must be narrow. A dynamic path can name any wrapper at run time, so
/// removing the multiplexer there would break `file_get_contents($url)` — the pair of tests is
/// what distinguishes "narrowed correctly" from "removed".
#[test]
fn test_dynamic_path_keeps_the_url_reader() {
    let dir = make_cli_test_dir("elephc_dynamic_path_reachability");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        "<?php $p = \"/etc/hosts\"; echo file_get_contents($p);",
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        user_asm.contains("__rt_file_get_contents_maybe_url"),
        "a dynamic path must still reach the URL multiplexer"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an executable exports its entry point and nothing else.
///
/// Every other global is `.globl` only so the user and runtime objects can find each other. On
/// Mach-O a `.globl` is an export, and an export is a dead-strip root, so leaving them unmarked
/// put the whole per-class machinery in the export trie. Marking them costs nothing at run time —
/// intra-image references are unaffected — and the regression is invisible without this check:
/// the program still runs, the binary is just larger.
#[test]
fn test_executable_marks_its_internal_symbols() {
    let dir = make_cli_test_dir("elephc_executable_visibility");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options("<?php echo 1;", &dir, 8_388_608, false, false);

    let directive = if cfg!(target_os = "macos") {
        ".private_extern "
    } else {
        ".hidden "
    };
    assert!(
        user_asm.contains(directive),
        "an executable should mark its internal globals as non-exported"
    );
    assert!(
        !user_asm.contains(&format!("{directive}_main\n")),
        "the entry point must stay exported"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an exception raised inside the SHARED `__toString` ladder still reaches the
/// caller's `catch`.
///
/// Moving the ladder into a synthetic function put a frame between the throw and the handler
/// that was not there when it was inlined, and nothing covered that: every existing test has a
/// `__toString` that returns. The three distinct call sites are load-bearing — with fewer the
/// ladder stays INLINE and the program never enters the helper, so the test would pass while
/// proving nothing. Measured that way first, and the emitted assembly named the shared label
/// zero times.
#[test]
fn test_a_throw_inside_the_shared_string_ladder_is_still_catchable() {
    let out = compile_and_run(
        r#"<?php
class Boom { public function __toString(): string { throw new RuntimeException("boom"); } }
class Fine { public function __toString(): string { return "fine"; } }
function first(mixed $v): void { echo $v; }
function second(mixed $v): void { echo $v; }
function third(mixed $v): void { echo $v; }
first(new Fine());
echo "|";
try {
    second(new Boom());
    echo "not-reached";
} catch (RuntimeException $e) {
    echo "caught:", $e->getMessage();
}
echo "|";
third(new Fine());
"#,
    );
    assert_eq!(out, "fine|caught:boom|fine");
}

/// Verifies the `__toString` dispatch ladder is emitted once for a program with several
/// boxed-`Mixed` string contexts, and stays inline for a program with one.
///
/// The ladder carries one arm per class publishing `__toString` and used to be copied at every
/// site: measured on `examples/iterators`, 12 sites of 592 IDENTICAL lines, 19.3% of the emitted
/// assembly. Sharing it costs a call, so a single-site program must keep the inline form —
/// without that second direction the change made `fizzbuzz` GROW, from 13 588 to 15 528 bytes of
/// `__text`, by paying for a helper body to remove one copy of itself.
#[test]
fn test_mixed_string_ladder_is_shared_only_when_several_sites_use_it() {
    const CLASSES: &str = r#"<?php
class Stamp { public function __toString(): string { return "S"; } }
function pick(int $i): mixed { return $i === 0 ? new Stamp() : $i; }
"#;

    let dir = make_cli_test_dir("elephc_shared_mixed_string_many");
    let many = format!("{CLASSES}$a = pick(0); $b = pick(1); $c = pick(2);\necho $a; echo $b; echo $c;\n");
    let (many_asm, _runtime_asm, _libraries) =
        compile_source_to_asm_with_options(&many, &dir, 8_388_608, false, false);
    assert_eq!(
        many_asm.matches("_eir_shared_mixed_echo:").count(),
        1,
        "the shared echo helper must be defined exactly once"
    );
    assert!(
        many_asm.matches("_eir_shared_mixed_echo").count() >= 4,
        "each of the three sites must call the helper it shares"
    );
    let _ = fs::remove_dir_all(&dir);

    let dir = make_cli_test_dir("elephc_shared_mixed_string_one");
    let one = format!("{CLASSES}$a = pick(0);\necho $a;\n");
    let (one_asm, _runtime_asm, _libraries) =
        compile_source_to_asm_with_options(&one, &dir, 8_388_608, false, false);
    assert!(
        !one_asm.contains("_eir_shared_mixed_echo"),
        "a single string context must stay inline rather than pay for a helper body"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the `count()` countable guard is emitted once for a program with several boxed
/// `count()` sites, and stays inline for a program with one.
///
/// The guard is seven tag comparisons and seven raises. The first version of this fix inlined
/// it and cost 292 lines of assembly PER SITE — measured on this exact five-site shape, 4 102
/// lines without the guard against 5 563 with it — which is why it was reverted rather than
/// kept. Shared, the same program is 4 457.
///
/// The single-site direction is not symmetry for its own sake: measured, one site is 4 191
/// inline against 4 201 shared, so sharing there would make the program grow.
#[test]
fn test_count_guard_is_shared_only_when_several_sites_use_it() {
    const PICK: &str = r#"<?php
function pick(int $i): mixed { return $i === 0 ? [1,2,3] : $i; }
"#;

    let dir = make_cli_test_dir("elephc_shared_count_guard_many");
    let many = format!(
        "{PICK}$a = pick(0); $b = pick(1); $c = pick(2);\necho count($a), count($b), count($c);\n"
    );
    let (many_asm, _runtime_asm, _libraries) =
        compile_source_to_asm_with_options(&many, &dir, 8_388_608, false, false);
    assert_eq!(
        many_asm.matches("_eir_shared_count_guard:").count(),
        1,
        "the shared count guard must be defined exactly once"
    );
    assert!(
        many_asm.matches("_eir_shared_count_guard").count() >= 4,
        "each of the three sites must call the guard it shares"
    );
    let _ = fs::remove_dir_all(&dir);

    let dir = make_cli_test_dir("elephc_shared_count_guard_one");
    let one = format!("{PICK}$a = pick(0);\necho count($a);\n");
    let (one_asm, _runtime_asm, _libraries) =
        compile_source_to_asm_with_options(&one, &dir, 8_388_608, false, false);
    assert!(
        !one_asm.contains("_eir_shared_count_guard"),
        "a single count() site must stay inline rather than pay for a helper body"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `count()`'s TypeError names every non-countable type the way reference PHP does,
/// from inside the SHARED guard.
///
/// The seven call sites are load-bearing twice over. Below the threshold the guard is inlined
/// and the throw never crosses a helper frame, so the test would exercise the wrong path; and
/// a `foreach` over seven values is ONE site, which is how the first version of this probe
/// silently measured the inline form — the emitted assembly named the shared label zero times.
///
/// The boolean is named by its VALUE. `false given` / `true given`, never `bool given`, is the
/// detail that makes the bool arm a two-way branch on the payload rather than one more tag
/// comparison.
#[test]
fn test_count_type_error_names_every_type_from_the_shared_guard() {
    let out = compile_and_run(
        r#"<?php
function pick(int $i): mixed {
    if ($i === 0) { return false; }
    if ($i === 1) { return true; }
    if ($i === 2) { return 7; }
    if ($i === 3) { return 1.5; }
    if ($i === 4) { return "s"; }
    if ($i === 5) { return null; }
    return [1, 2, 3];
}
$a = pick(0); $b = pick(1); $c = pick(2); $d = pick(3);
$e = pick(4); $f = pick(5); $g = pick(6);
try { echo count($a); } catch (TypeError $t) { echo $t->getMessage(); }
echo "\n";
try { echo count($b); } catch (TypeError $t) { echo $t->getMessage(); }
echo "\n";
try { echo count($c); } catch (TypeError $t) { echo $t->getMessage(); }
echo "\n";
try { echo count($d); } catch (TypeError $t) { echo $t->getMessage(); }
echo "\n";
try { echo count($e); } catch (TypeError $t) { echo $t->getMessage(); }
echo "\n";
try { echo count($f); } catch (TypeError $t) { echo $t->getMessage(); }
echo "\n";
try { echo count($g); } catch (TypeError $t) { echo $t->getMessage(); }
"#,
    );
    let prefix = "count(): Argument #1 ($value) must be of type Countable|array,";
    assert_eq!(
        out,
        format!(
            "{prefix} false given\n{prefix} true given\n{prefix} int given\n{prefix} float \
             given\n{prefix} string given\n{prefix} null given\n3"
        )
    );
}

/// Verifies a function whose only use of the reserved nested-call register is an inlined
/// `__toString` ladder saves that register before writing it.
///
/// The register is callee-saved and outside the allocator's tracking, so the frame reserves a
/// save slot from an explicit predicate — one that tested for a Mixed-receiver METHOD call and
/// nothing else. A string context is not a method call, so `function show(mixed $v) { echo $v; }`
/// emitted `mov x19, x1` under a prologue that saved nothing, breaking the calling convention
/// the same way issue #511 did. No caller shape was found that turns this into a wrong answer —
/// the receiver is established after argument lowering, which closes the obvious window — so
/// this guards the convention itself rather than a reproduced failure.
#[test]
fn test_string_context_function_saves_the_nested_call_register() {
    let dir = make_cli_test_dir("elephc_string_context_nested_reg");
    let (user_asm, _runtime_asm, _libraries) = compile_source_to_asm_with_options(
        r#"<?php
class Stamp { public function __toString(): string { return "S"; } }
function show(mixed $v): void { echo $v; }
show(new Stamp());
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let body = user_asm
        .split("@fn name=show")
        .nth(1)
        .expect("the show function must be emitted")
        .split("@endfn")
        .next()
        .expect("the show function must be terminated");
    let register = if cfg!(target_arch = "x86_64") { "r12" } else { "x19" };
    let first_write = body
        .find(&format!("mov {register},"))
        .or_else(|| body.find(&format!("mov {register} ,")))
        .expect("the inlined ladder must move the receiver into the nested-call register");
    // The contract is that the register REACHES a save slot before the ladder writes it — not
    // which slot. The x86 needle used to demand `[rbp - 8]` exactly, while the AArch64 ones were
    // already offset-agnostic; the allocator picks the frame offset, and it is `[rbp - 32]` for
    // this program, so the assertion failed on a prologue that saves r12 correctly. Only the
    // HOST arch's branch ever runs — `compile_source_to_asm_with_options` emits for the host —
    // which is why an x86-only needle could rot without any macOS or CI-aarch64 run noticing.
    let saved_before = body[..first_write].lines().any(|line| {
        let line = line.trim();
        line.starts_with(&format!("stur {register},"))
            || line.starts_with(&format!("str {register},"))
            || line.starts_with(&format!("push {register}"))
            || (line.starts_with("mov QWORD PTR [rbp -")
                && line.ends_with(&format!(", {register}")))
    });
    assert!(
        saved_before,
        "the nested-call register must be saved before the ladder overwrites it:\n{body}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the scope-cleanup destructor for a resource kind travels with the builtin that is
/// its only producer.
///
/// The `__rt_mixed_free_deep` arms for `popen` pipes and `opendir` streams were the sole
/// reference to `__rt_pclose` and `__rt_closedir`, so every binary imported `pclose`, `closedir`,
/// `globfree` and `close` to release handles it could not open — the four libc imports a trivial
/// program had apart from the `getrlimit` stack probe.
///
/// This is the direction the behaviour tests cannot see. `test_opendir_auto_closed_on_scope_exit`
/// asserts the program prints `done` and passes just as well with the arm missing: a leaked
/// descriptor in a process about to exit changes nothing observable. So the arm's PRESENCE for a
/// program that opens a directory is checked here, on the emitted runtime, and not left to a
/// functional test that would stay green while the handle leaked.
///
/// PARKED on #815. MEASURED on this branch: `<?php echo 1;` still imports `_pclose`, `_closedir`
/// and `_globfree` — three of its seven undefined symbols — so the verdict below is RIGHT and the
/// property is lost. The two labels it names cannot come back either: they were replaced by one
/// `__rt_mixed_free_deep_resource_registry` arm, which is emitted unconditionally. Restoring the
/// property and rewriting the assertion are one piece of work, and a green assertion over a lost
/// property would be worse than this red one.
#[test]
#[ignore = "#815: the per-kind destructor arms became one unconditional registry arm"]
fn test_resource_destructors_follow_the_builtin_that_produces_them() {
    // The arm LABEL, not the helper symbol: `__rt_pclose` and `__rt_closedir` are defined
    // unconditionally in the runtime and it is the linker that drops an unreferenced body, so
    // their names are in the assembly either way. The ladder arm is what makes them reachable.
    for (label, source, expects_popen_arm, expects_dir_arm) in [
        ("plain", "<?php echo 1;", false, false),
        ("popen", "<?php $p = popen(\"printf x\", \"r\"); echo fread($p, 4);", true, false),
        ("opendir", "<?php $h = opendir(\".\"); readdir($h);", false, true),
    ] {
        let dir = make_cli_test_dir("elephc_resource_destructor_reachability");
        let (_user_asm, runtime_asm, _required_libraries) =
            compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);

        assert_eq!(
            runtime_asm.contains("__rt_mixed_free_deep_resource_popen"),
            expects_popen_arm,
            "{label}: the popen destructor arm must be emitted exactly when popen() is lowered"
        );
        assert_eq!(
            runtime_asm.contains("__rt_mixed_free_deep_resource_dir"),
            expects_dir_arm,
            "{label}: the directory destructor arm must be emitted exactly when opendir() is \
             lowered"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
