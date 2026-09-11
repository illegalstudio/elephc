//! Purpose:
//! Integration coverage for top-level compile/native dispatch and compiler output modes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Native help and managed-PCRE2 recovery diagnostics are exercised through subprocesses.
//! - Non-link modes must remain independent of installed native artifacts.

use crate::support::*;

/// A PHP program broad enough to drag most user-codegen emitters into the
/// assembly: closures behind runtime callbacks, `call_user_func_array`, static
/// properties on a late-bound receiver, generators, fibers, string building,
/// and the array helpers that invoke a callable.
const CTX_USER_CODEGEN_FIXTURE: &str = r#"<?php
class Holder {
    public static int $count = 0;
    public static string $label = 'x';
    public static function bump(int $by): int { static::$count += $by; return static::$count; }
}
class Child extends Holder { public static int $count = 100; }

function apply(callable $f, array $xs): array { return array_map($f, $xs); }

$nums = [5, 3, 9, 1];
usort($nums, fn($a, $b) => $a <=> $b);
$sum = array_reduce($nums, fn($c, $v) => $c + $v, 0);
$even = array_filter($nums, fn($v) => $v % 2 === 0);
array_walk($nums, function ($v) use (&$sum) { $sum += $v; });
$doubled = apply(fn($v) => $v * 2, $nums);

$args = [2];
$viaCufa = call_user_func_array([Holder::class, 'bump'], $args);
$viaChild = Child::bump(3);

$gen = (function () { foreach ([1, 2, 3] as $v) { yield $v; } })();
$fromGen = 0;
foreach ($gen as $v) { $fromGen += $v; }

$fiber = new Fiber(function (int $seed) { Fiber::suspend($seed + 1); return $seed + 2; });
$suspended = $fiber->start(7);
$fiber->resume();

$text = sprintf('%s-%d', Holder::$label, $sum) . implode(',', $doubled);
echo strlen($text), count($even), $viaCufa, $viaChild, $fromGen, $suspended, $fiber->getReturn();
"#;

/// The reserved ctx register is a WHOLE-PROGRAM contract, not a runtime-only one.
///
/// The runtime audit (`x86_64_ctx_runtime_never_scratches_the_ctx_register`) scans
/// the generated runtime and nothing else, so USER codegen was free to borrow the
/// register — and did. `runtime_callable_invoker` used r14 as its tag register and
/// then called the PHP callable through it, so every `usort`/`array_filter`/
/// `array_reduce`/`array_walk`/`array_any` with a closure ran its callback with a
/// type tag where the context pointer belongs, and died on the first heap access.
/// Five SIGSEGVs on linux-x86_64 that no local check saw: the host suite compiles
/// for AArch64, and the runtime audit does not read user code.
///
/// This compiles a deliberately broad program for x86_64 and applies the same
/// sanctioned-shape rule to what comes out.
/// Removes every `[...]` memory operand that addresses through `register`.
///
/// What remains is the instruction's register operands, which is where a reserved register
/// can actually be lost. `cmp rsp, QWORD PTR [r14 + 128]` becomes `cmp rsp, QWORD PTR`,
/// naming r14 nowhere — it only read through it. `mov r14, rax` is untouched and still
/// names it, because that one overwrites the context pointer.
fn strip_memory_operands_using(line: &str, register: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        let operand = &rest[open..open + close + 1];
        out.push_str(&rest[..open]);
        // Keep an operand that does NOT use the register: it may still name it elsewhere.
        if !operand
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| token == register)
        {
            out.push_str(operand);
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// THE ROLE RULE ACCEPTS EVERY CTX ACCESS AND STILL REFUSES EVERY BORROW.
///
/// The scan below judges whether `r14` survives once the memory operands that merely
/// address through it are removed. That rule replaced an enumerated allowlist of
/// mnemonics, which called `cmp rsp, QWORD PTR [r14 + N]` a violation seventeen times the
/// moment the stack guard started reading this context's floor — a correct instruction the
/// list had no entry for.
///
/// A looser rule is worthless, so this pins both directions on shapes that actually occur.
/// Note the last refusal: `mov r14, QWORD PTR [rax]` installs whatever `rax` held as the
/// context pointer, and the old allowlist SANCTIONED it, because it began with
/// `mov r14, QWORD PTR [`. Widening the rule closed that hole rather than opening one.
#[test]
fn the_ctx_register_role_rule_separates_access_from_borrowing() {
    let reads_through_it = [
        "cmp rsp, QWORD PTR [r14 + 128]",
        "mov rax, QWORD PTR [r14 + 72]",
        "lea r8, [r14 + 4096]",
        "mov QWORD PTR [r14 + 176], 0",
        "add QWORD PTR [r14 + 144], 1",
    ];
    for line in reads_through_it {
        let remainder = strip_memory_operands_using(line, "r14");
        let still_named = remainder
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| matches!(token, "r14" | "r14d" | "r14w" | "r14b"));
        assert!(
            !still_named,
            "`{line}` only reads per-context state through r14; the rule must accept it \
             (left `{remainder}`)"
        );
    }

    let borrows_it = [
        "mov r14, rax",
        "xor r14d, r14d",
        "add r14, 8",
        "movzx r14d, BYTE PTR [rsi]",
        // The one the previous allowlist waved through.
        "mov r14, QWORD PTR [rax]",
    ];
    for line in borrows_it {
        let remainder = strip_memory_operands_using(line, "r14");
        let still_named = remainder
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| matches!(token, "r14" | "r14d" | "r14w" | "r14b"));
        assert!(
            still_named,
            "`{line}` overwrites the context pointer; the rule must still catch it \
             (left `{remainder}`)"
        );
    }
}

#[test]
fn test_cli_rt_ctx_user_codegen_never_scratches_the_ctx_register() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_user_codegen");
    let php_path = dir.join("main.php");
    fs::write(&php_path, CTX_USER_CODEGEN_FIXTURE).expect("failed to write the fixture");

    let output = elephc_cli_command(&dir)
        .args(["--target", "linux-x86_64"])
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile the user-codegen fixture");
    assert!(
        output.status.success(),
        "--rt-ctx --target linux-x86_64 --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read the emitted assembly");
    let mut offenders: Vec<String> = Vec::new();
    for raw in asm.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('.') || line.ends_with(':')
        {
            continue;
        }
        let line = line.split('#').next().unwrap_or(line).trim();
        let mentions_r14 = line
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| matches!(token, "r14" | "r14d" | "r14w" | "r14b"));
        if !mentions_r14 {
            continue;
        }
        // THE RULE IS ABOUT THE REGISTER'S ROLE, NOT THE MNEMONIC. Reading per-context
        // state THROUGH r14 is the whole point of ctx mode, and the set of instructions
        // that do it grows: routing the stack guard added `cmp rsp, QWORD PTR [r14 + N]`,
        // which an enumerated allowlist of `mov`/`lea` called a violation 17 times. What
        // must never happen is r14 becoming a DESTINATION — that is what loses the
        // context. So strip the memory operands that merely address through it, and judge
        // what is left.
        let without_ctx_addressing = strip_memory_operands_using(line, "r14");
        let still_names_r14 = without_ctx_addressing
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| matches!(token, "r14" | "r14d" | "r14w" | "r14b"));
        let sanctioned = !still_names_r14
            // The publication itself, and the whole-register save/restore pair a foreign
            // entry uses. A restore must come from the frame, not from an arbitrary
            // pointer: `mov r14, QWORD PTR [rax]` would install whatever rax held.
            || line == "push r14"
            || line == "pop r14"
            || line.starts_with("lea r14, [rip + _rt_ctx]")
            || line.starts_with("mov r14, QWORD PTR [rbp")
            || line.starts_with("mov r14, QWORD PTR [rsp")
            // r14 as a SOURCE — saving it, or comparing against it — never loses it.
            || line.ends_with(", r14");
        if !sanctioned {
            offenders.push(line.to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "--rt-ctx user codegen borrowed r14, the reserved context register \
         ({} offenders). Compiled PHP reads per-context state through it, so a \
         callback invoked with a borrowed r14 faults on its first heap access:\n{}",
        offenders.len(),
        offenders.join("\n")
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `--rt-ctx` selects the ctx-register runtime mode: the emitted main entry
/// must install the per-context state pointer by calling `__rt_ctx_init`
/// before any user statement runs.
#[test]
fn test_cli_rt_ctx_main_calls_ctx_init() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_main_init");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").expect("failed to write the rt-ctx fixture");

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile rt-ctx program");
    assert!(
        output.status.success(),
        "elephc --rt-ctx --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read rt-ctx assembly");
    assert!(
        asm.contains("bl __rt_ctx_init") || asm.contains("call __rt_ctx_init"),
        "--rt-ctx main prologue must call __rt_ctx_init:\n{}",
        asm.lines().take(40).collect::<Vec<_>>().join("\n")
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A full `--rt-ctx` build links and runs end to end: the ctx-routed heap
/// allocator must serve real allocations (array + string) driven through the
/// reserved ctx register, proving the mode executable on the host target.
/// The program also reads `$argc`/`$argv`: the ctx-init helper runs before
/// argc/argv are spilled in the main prologue, so a clobber there would
/// surface here as garbage arguments (spike review, B5).
#[test]
fn test_cli_rt_ctx_binary_runs_and_allocates() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_binary_run");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\n$a = [10, 20, 30];\n$s = 'x' . 'y' . 'z';\necho count($a) + strlen($s);\n",
    )
    .expect("failed to write the rt-ctx run fixture");

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile rt-ctx run fixture");
    assert!(
        output.status.success(),
        "elephc --rt-ctx failed to compile and link: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bin = dir.join("main");
    let run = std::process::Command::new(&bin)
        .output()
        .expect("failed to run the rt-ctx binary");
    assert!(
        run.status.success(),
        "rt-ctx binary exited with {}: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "6",
        "rt-ctx allocation-driven program must print 6 (count 3 + strlen 3)"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `__rt_ctx_init` runs BEFORE argc/argv are spilled in the main prologue; the
/// helper's pinned clobber contract (ctx register only, no argument
/// registers) must hold, or a ctx build reads garbage arguments. This drives
/// an argument through `$argv[1]` end to end.
#[test]
fn test_cli_rt_ctx_preserves_argv_across_ctx_init() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_argv");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\necho isset($argv[1]) ? $argv[1] : 'none';\n",
    )
    .expect("failed to write the rt-ctx argv fixture");

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile rt-ctx argv fixture");
    assert!(
        output.status.success(),
        "elephc --rt-ctx argv compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bin = dir.join("main");
    let run = std::process::Command::new(&bin)
        .arg("hello-ctx")
        .output()
        .expect("failed to run the rt-ctx argv binary");
    assert!(
        run.status.success(),
        "rt-ctx argv binary crashed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "hello-ctx",
        "ctx-init must not clobber argc/argv before the prologue spills them"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The ctx-mode allocator must RECYCLE: a loop whose cumulative allocation
/// volume exceeds the default heap several times over can only complete
/// through free-list/small-bin reuse. This is the codified form of the
/// partial-routing trap the spike found (frees landing on an unread global
/// free list exhausted the heap) — under a broken routing this test fails
/// with "heap memory exhausted" (spike review, D3).
#[test]
fn test_cli_rt_ctx_heap_recycling_survives_cumulative_allocations() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_recycle");
    let php_path = dir.join("main.php");
    // Each iteration allocates an 8-element array and a short string, then
    // releases both at the next iteration's reassignment. 600k iterations ×
    // ~100 bytes ≈ 60 MB through an 8 MB heap: >7x reuse is mandatory.
    fs::write(
        &php_path,
        "<?php\n$sum = 0;\nfor ($i = 0; $i < 600000; $i++) {\n    $a = [1, 2, 3, 4, $i, $i + 1, $i + 2, $i + 3];\n    $s = 'v' . $i;\n    $sum = ($sum + count($a) + strlen($s)) % 1000003;\n}\necho $sum;\n",
    )
    .expect("failed to write the rt-ctx recycle fixture");

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile rt-ctx recycle fixture");
    assert!(
        output.status.success(),
        "elephc --rt-ctx recycle compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bin = dir.join("main");
    let run = std::process::Command::new(&bin)
        .output()
        .expect("failed to run the rt-ctx recycle binary");
    assert!(
        run.status.success(),
        "rt-ctx recycle binary failed (recycling broken?): {}",
        String::from_utf8_lossy(&run.stderr)
    );
    // Deterministic output: the same program must print the same value under
    // the legacy addressing mode (golden cross-check, D7).
    let ctx_out = String::from_utf8_lossy(&run.stdout);
    assert!(!ctx_out.trim().is_empty(), "recycle program printed nothing");

    // Golden: recompile the identical program WITHOUT --rt-ctx and compare.
    let legacy_php = dir.join("legacy.php");
    fs::copy(&php_path, &legacy_php).expect("failed to copy the legacy fixture");
    let output = elephc_cli_command(&dir)
        .arg(&legacy_php)
        .output()
        .expect("failed to compile legacy recycle fixture");
    assert!(
        output.status.success(),
        "elephc legacy recycle compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let legacy_run = std::process::Command::new(dir.join("legacy"))
        .output()
        .expect("failed to run the legacy recycle binary");
    assert_eq!(
        ctx_out,
        String::from_utf8_lossy(&legacy_run.stdout),
        "ctx and legacy modes must produce identical output for the same program"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Fiber and generator bodies run on zero-initialized fiber stacks, so the first
/// switch restores a zeroed ctx register; the fiber entry trampoline must
/// re-publish the per-context pointer or every allocation inside the coroutine
/// dereferences NULL. Both coroutines allocate, so both would crash without it.
#[test]
fn test_cli_rt_ctx_fibers_and_generators_re_publish_ctx() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_fiber_ctx");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\n\
         function gen($n) {\n\
             for ($i = 0; $i < $n; $i++) { $a = [$i, $i + 1, $i + 2]; yield $a[0] + $a[1]; }\n\
             return 'done';\n\
         }\n\
         $total = 0;\n\
         foreach (gen(1000) as $v) { $total = ($total + $v) % 999983; }\n\
         $f = new Fiber(function () {\n\
             $x = [4, 5, 6];\n\
             Fiber::suspend(count($x));\n\
             return 7;\n\
         });\n\
         $r1 = $f->start();\n\
         $r2 = $f->resume();\n\
         echo $total + $r1 + $r2;\n",
    )
    .expect("failed to write the rt-ctx fiber fixture");

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile rt-ctx fiber fixture");
    assert!(
        output.status.success(),
        "elephc --rt-ctx fiber compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bin = dir.join("main");
    let run = std::process::Command::new(&bin)
        .output()
        .expect("failed to run the rt-ctx fiber binary");
    assert!(
        run.status.success(),
        "rt-ctx fiber binary crashed ({}): {}",
        run.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&run.stderr)
    );
    // The running total folds (2i+1) modulo 999983 per iteration, landing on 10;
    // + 3 (suspend count) + 7 (fiber return) = 20. Legacy and ctx builds agree.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "20",
        "rt-ctx generator + fiber program must print 20"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A runtime helper that borrows the reserved ctx register corrupts the state
/// pointer for every ctx access it makes — and `__rt_wordwrap` did exactly that
/// on AArch64 (it kept its output cursor in x28), so the `__rt_concat_publish`
/// it calls at the end wrote the new scratch offset into its own result buffer
/// instead of into `_rt_ctx`. The offset then never advanced and every later
/// concat-backed result reused the same bytes.
///
/// The witness needs results that are NOT copied to the heap first: consumed
/// inside one expression they stay in the concat arena, so a stale offset makes
/// the second and third `wordwrap()` echo the FIRST one's text.
#[test]
fn test_cli_rt_ctx_concat_backed_results_do_not_overwrite_each_other() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_concat_reuse");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\n\
         echo wordwrap(\"aaaa bbbb\", 4, \"\\n\", true) . \"[\" .\n\
              wordwrap(\"wwww xxxx\", 4, \"\\n\", true) . \"]\" .\n\
              wordwrap(\"1111 2222\", 4, \"\\n\", true);\n",
    )
    .expect("failed to write the rt-ctx concat-reuse fixture");

    // One mode now: this used to run the fixture twice to compare ctx against legacy, and
    // the comparison is what went away with the flag, not the witness.
    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile the concat-reuse fixture");
    assert!(
        output.status.success(),
        "concat-reuse compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let run = std::process::Command::new(dir.join("main"))
        .output()
        .expect("failed to run the concat-reuse binary");
    assert!(
        run.status.success(),
        "concat-reuse binary crashed ({}): {}",
        run.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&run.stderr)
    );
    // php's own output for this program, byte for byte.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "aaaa\nbbbb[wwww\nxxxx]1111\n2222",
        "the build reused the concat arena across wordwrap() results"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// THE DEFAULT BUILD IS THE CTX BUILD. There is no other one.
///
/// This test used to assert the opposite — that a plain `elephc app.php` stayed on legacy
/// symbol addressing — which was true for as long as `--rt-ctx` existed to select the
/// other arm. Inverting it rather than deleting it keeps the fact pinned from the same
/// place: a regression that reintroduced legacy addressing as the default would otherwise
/// pass every test in this file.
#[test]
fn test_cli_default_main_installs_the_context() {
    let dir = make_cli_test_dir("elephc_cli_default_ctx_init");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").expect("failed to write the default fixture");

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile default program");
    assert!(
        output.status.success(),
        "elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read default assembly");
    assert!(
        asm.contains("__rt_ctx_init"),
        "the default prologue must install the per-context state pointer:\n{}",
        asm.lines().take(40).collect::<Vec<_>>().join("\n")
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `--rt-ctx` IS REJECTED, NOT IGNORED.
///
/// A build script that still passes the flag must learn it is gone. Silently accepting it
/// would be worse than either keeping or removing it: the script would go on believing it
/// selects something. The rejection lives here rather than in a unit test because an
/// unknown flag calls `fail()`, which exits the process.
#[test]
fn test_cli_rejects_the_removed_rt_ctx_flag() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_removed");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").expect("failed to write the fixture");

    let output = elephc_cli_command(&dir)
        .arg("--rt-ctx")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc");
    assert!(!output.status.success(), "--rt-ctx must not be accepted any more");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unknown flag: --rt-ctx"),
        "the rejection must name the flag so a caller knows what to remove, got:\n{stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A top-level-only program still produces one exact `{main}` frame when run
/// through the real compile/control-channel/monitor pipeline.
#[test]
fn test_cli_monitor_profiles_a_top_level_only_program() {
    let dir = make_cli_test_dir("elephc_cli_monitor_main_only");
    fs::write(
        dir.join("top.php"),
        "<?php\n$sum = 0; for ($i = 0; $i < 200000; $i = $i + 1) { $sum = ($sum + $i) % 100003; } echo $sum;\n",
    )
    .expect("failed to write the top-level monitoring fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "top.php",
            "--out",
            "top.prof.json",
            "--save",
            "top.exact.json",
        ])
        .output()
        .expect("failed to run exact top-level monitor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "top-level monitor should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("exact profile"), "{stdout}");
    assert!(stdout.contains("{main}"), "the exact root is missing: {stdout}");

    let raw = fs::read_to_string(dir.join("top.prof.json"))
        .expect("top-level monitor should write its Speedscope export");
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    assert!(
        doc["shared"]["frames"]
            .as_array()
            .expect("frames")
            .iter()
            .any(|frame| frame["name"] == "{main}"),
        "the export must preserve the exact root: {raw}"
    );
    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("top.exact.json")).expect("saved exact graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    assert_eq!(nodes.len(), 1, "a top-level-only run has one frame: {exact}");
    assert_eq!(nodes[0]["name"], "{main}");
    assert_eq!(nodes[0]["call_count"], 1, "root must enter and exit once");
    assert!(
        exact["edges"].as_array().expect("exact edges").is_empty(),
        "the root must not call itself: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// End-to-end `elephc monitor`: compiles a busy fixture, activates exact
/// instrumentation through the control channel, and writes a two-view
/// Speedscope document whose frames are PHP names — not EIR block labels or
/// runtime helpers in the folded view.
///
/// The export matters as much as the table. `--out` and `--pprof` were once
/// wired only to the sampled capture, so when the exact profile became the
/// default they wrote nothing at all — silently, including for the CI
/// regression gate, which is documented as `--out` a baseline and `--baseline`
/// it back. A test that only read the table would not have noticed.
#[test]
fn test_cli_monitor_writes_php_level_speedscope_profile() {
    let dir = make_cli_test_dir("elephc_cli_monitor");
    // The hot function is RECURSIVE on purpose: a self-recursive body cannot be
    // fully inlined away, so its frame is guaranteed in the samples — the test
    // must not depend on the best-effort inlined-frame recovery, whose address
    // bucketing varies run to run.
    fs::write(
        dir.join("busy.php"),
        "<?php\nfunction burn(int $depth) { $n = 0; for ($i = 0; $i < 2000000; $i = $i + 1) { $n = ($n + $i) % 1000003; } if ($depth > 0) { $n = ($n + burn($depth - 1)) % 1000003; } return $n; }\necho burn(4);\n",
    )
    .expect("failed to write the monitor fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "busy.php",
            "--out",
            "busy.prof.json",
            "--save",
            "busy.exact.json",
        ])
        .output()
        .expect("failed to run elephc monitor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "monitor should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("exact profile") && stdout.contains("{main}") && stdout.contains("burn"),
        "the exact table should contain its root and the PHP function: {stdout}"
    );

    let raw = fs::read_to_string(dir.join("busy.prof.json"))
        .expect("monitor should write the speedscope file");
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    let profiles = doc["profiles"].as_array().expect("profiles array");
    assert_eq!(profiles.len(), 2, "one folded view and one why view");
    for profile in profiles {
        let weights: u64 = profile["weights"]
            .as_array()
            .expect("weights")
            .iter()
            .map(|w| w.as_u64().expect("integer weight"))
            .sum();
        assert_eq!(
            weights,
            profile["endValue"].as_u64().expect("endValue"),
            "weights must partition the profile total"
        );
    }
    let frames = doc["shared"]["frames"].as_array().expect("frames");
    // `burn` may still appear as `burn (inlined)` for partially inlined
    // shallow calls; either spelling proves the PHP-level attribution worked.
    assert!(
        frames
            .iter()
            .any(|f| f["name"].as_str().is_some_and(|n| n.starts_with("burn"))),
        "frames should carry the demangled PHP name: {raw}"
    );
    assert!(
        !frames
            .iter()
            .any(|f| f["name"].as_str().is_some_and(|n| n.contains("eir_"))),
        "no EIR block label may leak into the profile: {raw}"
    );

    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("busy.exact.json")).expect("saved exact graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    let main = nodes.iter().position(|node| node["name"] == "{main}").unwrap();
    let burn = nodes.iter().position(|node| node["name"] == "burn").unwrap();
    assert_eq!(nodes[main]["call_count"], 1, "root must enter exactly once");
    assert!(
        exact["edges"].as_array().expect("exact edges").iter().any(|edge| {
            edge["from"] == main && edge["to"] == burn && edge["count"] == 1
        }),
        "the ordinary call must be attributed from {{main}} to burn: {exact}"
    );
    assert!(
        !exact["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["to"] == main),
        "no function may be misattributed as calling the root: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Shutdown output handlers and object destructors remain children of the
/// exact `{main}` frame instead of becoming disconnected graph roots.
#[test]
fn test_cli_monitor_keeps_shutdown_callbacks_under_main() {
    let dir = make_cli_test_dir("elephc_cli_monitor_shutdown_callbacks");
    fs::write(
        dir.join("shutdown.php"),
        r#"<?php
function shutdown_handler(string $contents, int $phase): string {
    $n = 0;
    for ($i = 0; $i < 10000; $i = $i + 1) { $n = $n + $i; }
    if ($phase < 0) { echo $n; }
    return $contents;
}
class CleanupProbe {
    public function __destruct() {
        $n = 0;
        for ($i = 0; $i < 10000; $i = $i + 1) { $n = $n + $i; }
    }
}
$probe = new CleanupProbe();
ob_start(shutdown_handler(...));
echo "profiled\n";
"#,
    )
    .expect("failed to write the shutdown monitoring fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "shutdown.php",
            "--out",
            "shutdown.prof.json",
            "--save",
            "shutdown.exact.json",
        ])
        .output()
        .expect("failed to monitor shutdown callbacks");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "shutdown monitor should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );

    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("shutdown.exact.json")).expect("saved shutdown graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    let edges = exact["edges"].as_array().expect("exact edges");
    let main = nodes.iter().position(|node| node["name"] == "{main}").unwrap();
    let handler = nodes
        .iter()
        .position(|node| node["name"] == "shutdown_handler")
        .unwrap();
    let destructor = nodes
        .iter()
        .position(|node| node["name"] == "CleanupProbe::__destruct")
        .unwrap();
    for child in [handler, destructor] {
        assert!(
            edges.iter().any(|edge| {
                edge["from"] == main && edge["to"] == child && edge["count"] == 1
            }),
            "shutdown PHP work must remain below {{main}}: {exact}"
        );
    }
    assert!(
        nodes[main]["inclusive"].as_u64().unwrap()
            > nodes[main]["exclusive"].as_u64().unwrap(),
        "the root must subtract shutdown callees from self time: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Runs one clean language-level termination fixture and verifies that monitor
/// receives the complete exact graph despite bypassed generated epilogues.
fn assert_clean_language_exit_profile(tag: &str, source: &str) {
    let dir = make_cli_test_dir(tag);
    fs::write(
        dir.join("exit.php"),
        source,
    )
    .expect("failed to write the clean-exit monitoring fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "exit.php",
            "--out",
            "exit.prof.json",
            "--save",
            "exit.exact.json",
        ])
        .output()
        .expect("failed to monitor a clean language exit");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "a clean language exit should publish a profile\nstdout: {stdout}\nstderr: {stderr}"
    );

    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("exit.exact.json")).expect("saved exit graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    let main = nodes.iter().position(|node| node["name"] == "{main}").unwrap();
    let child = nodes
        .iter()
        .position(|node| node["name"] == "before_exit")
        .unwrap();
    assert_eq!(nodes[main]["call_count"], 1);
    assert!(
        exact["edges"].as_array().expect("exact edges").iter().any(|edge| {
            edge["from"] == main && edge["to"] == child && edge["count"] == 1
        }),
        "the clean exit must publish the complete rooted graph: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `exit(0)` closes and publishes the active exact stack even though normal
/// generated function and main epilogues are bypassed.
#[test]
fn test_cli_monitor_profiles_a_clean_language_exit() {
    assert_clean_language_exit_profile(
        "elephc_cli_monitor_exit_zero",
        r#"<?php
function before_exit(): int {
    $n = 0;
    for ($i = 0; $i < 10000; $i = $i + 1) { $n = $n + $i; }
    return $n;
}
$value = before_exit();
if ($value < 0) { echo $value; }
exit(0);
"#,
    );
}

/// The zero-argument `die()` lowering publishes the same rooted exact graph as
/// the status-bearing `exit(0)` path.
#[test]
fn test_cli_monitor_profiles_clean_die_without_status() {
    assert_clean_language_exit_profile(
        "elephc_cli_monitor_die_no_status",
        r#"<?php
function before_exit(): int {
    $n = 0;
    for ($i = 0; $i < 10000; $i = $i + 1) { $n = $n + $i; }
    return $n;
}
$value = before_exit();
if ($value < 0) { echo $value; }
die();
"#,
    );
}

/// An uncaught generated PHP error bypasses every ordinary epilogue just like
/// `exit()`, so it must still close and publish the exact root and live callee.
#[test]
fn test_cli_monitor_profiles_an_uncaught_codegen_error() {
    let dir = make_cli_test_dir("elephc_cli_monitor_uncaught_codegen_error");
    // The negative-value recursive branch keeps the failing function out of the
    // inliner while `$argc` still takes the direct uncaught path at runtime.
    fs::write(
        dir.join("uncaught.php"),
        r#"<?php
function fail_uncaught(int $value): int {
    if ($value < 0) { return fail_uncaught(-$value); }
    return intdiv($value, $value - $value);
}
fail_uncaught($argc);
"#,
    )
    .expect("failed to write the uncaught-error monitoring fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "uncaught.php",
            "--out",
            "uncaught.prof.json",
            "--save",
            "uncaught.exact.json",
        ])
        .output()
        .expect("failed to monitor an uncaught generated error");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "an uncaught generated error should still publish a profile\nstdout: {stdout}\nstderr: {stderr}"
    );

    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("uncaught.exact.json"))
            .expect("saved uncaught-error graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    let main = nodes.iter().position(|node| node["name"] == "{main}").unwrap();
    let failing = nodes
        .iter()
        .position(|node| node["name"] == "fail_uncaught")
        .unwrap();
    assert_eq!(nodes[main]["call_count"], 1);
    assert!(
        exact["edges"].as_array().expect("exact edges").iter().any(|edge| {
            edge["from"] == main && edge["to"] == failing && edge["count"] == 1
        }),
        "the uncaught error must publish the complete rooted graph: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies both compiler version flags print the Cargo package version and exit successfully.
#[test]
fn test_cli_version_flags_report_package_version() {
    let dir = make_cli_test_dir("elephc_cli_version");
    let expected = format!("elephc {}\n", env!("CARGO_PKG_VERSION"));

    for flag in ["--version", "-V"] {
        let output = elephc_cli_command(&dir)
            .arg(flag)
            .output()
            .unwrap_or_else(|error| panic!("failed to run elephc {flag}: {error}"));
        assert!(output.status.success(), "elephc {flag} should succeed");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert!(output.stderr.is_empty(), "elephc {flag} should not write stderr");
    }

    let help = elephc_cli_command(&dir)
        .arg("--help")
        .output()
        .expect("failed to run elephc --help");
    assert!(help.status.success(), "elephc --help should succeed");
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains(&format!("Version: {}", env!("CARGO_PKG_VERSION"))));
    assert!(stdout.contains("-V, --version"));
    assert!(stdout.contains("name `{main}` for the top-level root"));

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies native help is handled before project discovery and bare native is a usage error.
#[test]
fn test_cli_native_help_and_bare_usage() {
    let dir = make_cli_test_dir("elephc_cli_native_help");

    let help = elephc_cli_command(&dir)
        .args(["native", "--help"])
        .output()
        .expect("failed to run elephc native --help");
    assert!(help.status.success(), "native help should succeed");
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("elephc native add"),
        "native help should print the command synopsis"
    );
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("elephc native prune"),
        "native help should include explicit cache pruning"
    );

    let bare = elephc_cli_command(&dir)
        .arg("native")
        .output()
        .expect("failed to run bare elephc native");
    assert!(!bare.status.success(), "bare native should be a usage error");
    let stderr = String::from_utf8_lossy(&bare.stderr);
    assert!(stderr.contains("missing native command"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("elephc native install"), "missing synopsis: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies read-only native commands preserve their captured stdout and health exit status.
#[test]
fn test_cli_native_read_only_commands_map_output_and_status() {
    let dir = make_cli_test_dir("elephc_cli_native_read_only");
    let cache = dir.join("native-cache-must-not-exist");

    let list = elephc_cli_command(&dir)
        .args(["native", "list"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run elephc native list");
    assert!(list.status.success(), "empty native list should succeed");
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("no native dependencies"),
        "unexpected list output: {}",
        String::from_utf8_lossy(&list.stdout)
    );

    let doctor = elephc_cli_command(&dir)
        .args(["native", "doctor"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run elephc native doctor");
    assert!(!doctor.status.success(), "doctor without a project should be unhealthy");
    assert!(
        String::from_utf8_lossy(&doctor.stdout).contains("summary: unhealthy")
            && String::from_utf8_lossy(&doctor.stdout).contains("cache size:")
            && String::from_utf8_lossy(&doctor.stdout).contains("stale staging summary:"),
        "unexpected doctor output: {}",
        String::from_utf8_lossy(&doctor.stdout)
    );
    assert!(!cache.exists(), "read-only commands must not create the native cache");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies explicit pruning is a successful no-op when no global native cache exists.
#[test]
fn test_cli_native_prune_empty_cache_is_noop() {
    let dir = make_cli_test_dir("elephc_cli_native_prune_empty");
    let cache = dir.join("native-cache-must-not-exist");
    let prune = elephc_cli_command(&dir)
        .args(["native", "prune"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run elephc native prune");
    assert!(prune.status.success(), "empty-cache prune should succeed");
    assert!(
        String::from_utf8_lossy(&prune.stdout).contains("removed stale artifacts: 0"),
        "unexpected prune output: {}",
        String::from_utf8_lossy(&prune.stdout)
    );
    assert!(!cache.exists(), "empty-cache prune must not create cache state");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies non-link output modes never require or create a managed native cache.
#[test]
fn test_cli_regex_non_link_modes_skip_native_resolution() {
    for mode in ["--check", "--emit-ir", "--emit-asm"] {
        let dir = make_cli_test_dir("elephc_cli_regex_non_link");
        let cache = dir.join("native-cache-must-not-exist");
        let php_path = dir.join("main.php");
        fs::write(&php_path, "<?php echo preg_match('/a/', 'a');").unwrap();

        let output = elephc_cli_command(&dir)
            .arg(mode)
            .arg(&php_path)
            .env("ELEPHC_NATIVE_CACHE", &cache)
            .output()
            .unwrap_or_else(|error| panic!("failed to run elephc {mode}: {error}"));
        assert!(
            output.status.success(),
            "elephc {mode} unexpectedly required native PCRE2: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !cache.exists(),
            "elephc {mode} must not create the managed native cache"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

/// Verifies a final regex link without a project fails with the frozen recovery command.
#[test]
fn test_cli_regex_final_link_requires_managed_pcre2_project() {
    let dir = make_cli_test_dir("elephc_cli_regex_requires_native");
    let cache = dir.join("native-cache-must-not-exist");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo preg_match('/a/', 'a');").unwrap();

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run final-link regex compilation");
    assert!(!output.status.success(), "regex link without a project must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("regex support requires managed native package pcre2"),
        "unexpected missing-project diagnostic: {stderr}"
    );
    assert!(stderr.contains("project: not found"), "missing project context: {stderr}");
    assert!(
        stderr.contains("recovery: cd --") && stderr.contains("elephc native add pcre2"),
        "missing copy-paste recovery command: {stderr}"
    );
    assert!(!cache.exists(), "failed compilation must not create the native cache");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a final `--with-curl` link without a project fails with the same recovery
/// style as PCRE2. The compiled program itself never calls a `curl_*` function — the
/// explicit `--with-curl` flag is what forces `elephc_curl` into the plan here, exercising
/// that override path directly rather than source-based detection — but planning it must
/// still emit the `curl` native requirement and never fall back to a system `-lcurl`.
#[test]
fn test_cli_with_curl_final_link_requires_managed_curl_project() {
    let dir = make_cli_test_dir("elephc_cli_curl_requires_native");
    let cache = dir.join("native-cache-must-not-exist");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-curl")
        .arg(&php_path)
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run --with-curl compilation");
    assert!(!output.status.success(), "--with-curl link without a project must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("curl support requires managed native package curl"),
        "unexpected missing-project diagnostic: {stderr}"
    );
    assert!(stderr.contains("project: not found"), "missing project context: {stderr}");
    assert!(
        stderr.contains("recovery: cd --") && stderr.contains("elephc native add curl"),
        "missing copy-paste recovery command: {stderr}"
    );
    assert!(!cache.exists(), "failed compilation must not create the native cache");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a final `--with-xml` link without a project fails with the same managed-package
/// recovery style as curl and PCRE2. The compiled program itself never names the xml
/// surface — the explicit `--with-xml` flag is what forces `elephc_xml` into the plan here —
/// but planning it must still emit the `libxml2` native requirement (the bridge's parser IS
/// the managed libxml2, reached through the catalog's shim) and never fall back to a system
/// `-lxml2`. The assertion matches the package half of the diagnostic only, so it holds for
/// whichever feature label the resolver prints in front of it.
#[test]
fn test_cli_with_xml_final_link_requires_managed_libxml2_project() {
    let dir = make_cli_test_dir("elephc_cli_xml_requires_native");
    let cache = dir.join("native-cache-must-not-exist");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-xml")
        .arg(&php_path)
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run --with-xml compilation");
    assert!(!output.status.success(), "--with-xml link without a project must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("support requires managed native package libxml2"),
        "unexpected missing-project diagnostic: {stderr}"
    );
    assert!(stderr.contains("project: not found"), "missing project context: {stderr}");
    assert!(
        stderr.contains("recovery: cd --") && stderr.contains("elephc native add libxml2"),
        "missing copy-paste recovery command: {stderr}"
    );
    assert!(
        !stderr.contains("-lxml2") && !stderr.contains("required Elephc bridge"),
        "the managed-package diagnostic must fire before any link attempt: {stderr}"
    );
    assert!(!cache.exists(), "failed compilation must not create the native cache");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--check` stops after type-checking and produces "Checked" output
/// without emitting any assembly (.s), object (.o), or binary files.
#[test]
fn test_cli_check_stops_after_typecheck() {
    let dir = make_cli_test_dir("elephc_cli_check");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
echo "ok";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--check")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --check");

    assert!(
        output.status.success(),
        "elephc --check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Checked"),
        "expected --check success output, got stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !dir.join("main.s").exists(),
        "--check should not emit assembly files"
    );
    assert!(
        !dir.join("main.o").exists(),
        "--check should not emit object files"
    );
    assert!(
        !dir.join("main").exists(),
        "--check should not emit binaries"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--emit-asm` writes a .s assembly file containing the `_main` label
/// but does NOT produce object or binary files.
#[test]
fn test_cli_emit_asm_writes_assembly_only() {
    let dir = make_cli_test_dir("elephc_cli_emit_asm");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
echo "ok";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --emit-asm");

    assert!(
        output.status.success(),
        "elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Emitted assembly"),
        "expected --emit-asm success output, got stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );

    let asm_path = dir.join("main.s");
    assert!(asm_path.exists(), "--emit-asm should write the .s file");
    let asm = fs::read_to_string(&asm_path).expect("failed to read emitted assembly");
    assert!(
        asm.contains("_main"),
        "expected emitted assembly to contain the program entry label"
    );
    assert!(
        !dir.join("main.o").exists(),
        "--emit-asm should not emit object files"
    );
    assert!(
        !dir.join("main").exists(),
        "--emit-asm should not emit binaries"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The teardown calls in main's epilogue must run on an aligned stack (x86_64).
///
/// System V AMD64 wants `rsp` 16-byte aligned AT the call. After `leave`, `rsp`
/// holds its entry value — already 8 past alignment — so every call emitted
/// after the frame restore is off by 8. The hand-written runtime helpers
/// tolerate that; compiled Rust does not, because an aligned SSE store to a
/// stack temporary faults.
///
/// It cost a CI shard on linux-x86_64 alone: `main` ran, printed its output, and
/// died in the profiler's exit dump — the last call before the exit syscall and
/// the first one made of Rust. AArch64 keeps `sp` aligned by construction, so
/// the same commit was green there and the failure looked like a profiler bug
/// for an afternoon.
///
/// Read from the assembly because that is where the property lives, and because
/// this host cannot execute the architecture that has it.
#[test]
fn test_cli_x86_64_epilogue_aligns_before_its_teardown_calls() {
    let dir = make_cli_test_dir("elephc_cli_x86_epilogue_align");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php\nfunction f(int $n): int { return $n + 1; }\necho f(1);\n").unwrap();

    let output = elephc_cli_command(&dir)
        .args(["--with-monitoring", "--target", "linux-x86_64", "--emit-asm"])
        .arg(&php_path)
        .output()
        .expect("failed to emit x86_64 assembly");
    assert!(
        output.status.success(),
        "cross-target --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("expected target assembly output");
    let lines: Vec<&str> = asm.lines().map(str::trim).collect();

    // Anchor on the teardown calls themselves, walk BACK to the frame restore
    // that precedes them, and require the realignment in between. Scanning
    // forward from `leave` and stopping at the first alignment was the version
    // that asserted nothing at all — it never reached a call.
    let teardown = lines
        .iter()
        .position(|line| line.starts_with("call elephc_probe_dump")
            || line.starts_with("call elephc_instr_dump"))
        .expect("a monitored build must emit its exit dump");
    let restore = lines[..teardown]
        .iter()
        .rposition(|line| *line == "leave")
        .expect("the exit dump follows main's frame restore");
    let between = &lines[restore + 1..teardown];
    assert!(
        between.iter().any(|line| line.starts_with("and rsp, -16")),
        "`{}` is called after `leave` with no realignment between them, so it \
         runs 8 bytes off the alignment the ABI promises it. Between:\n{}",
        lines[teardown],
        between.join("\n")
    );
    assert!(
        !between.iter().any(|line| line.starts_with("call ")),
        "nothing may be called between the frame restore and the realignment"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies cross-target `--emit-asm` stops before preparing a host-incompatible runtime object.
#[test]
fn test_cli_emit_asm_does_not_require_target_assembler() {
    let dir = make_cli_test_dir("elephc_cli_emit_cross_target_asm");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'cross-target';").unwrap();

    let target = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-aarch64"
    } else {
        "linux-x86_64"
    };
    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg(target)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run cross-target elephc CLI with --emit-asm");

    assert!(
        output.status.success(),
        "cross-target elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.join("main.s").exists(), "expected target assembly output");
    assert!(
        !dir.join("main.o").exists() && !dir.join("main").exists(),
        "cross-target --emit-asm must not assemble or link"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies plain `--web` assembly keeps the compact auto-start core while
/// pruning public session APIs and callable-handler machinery that user code
/// does not reference.
#[test]
fn test_cli_web_prunes_unused_session_surface_from_assembly() {
    let dir = make_cli_test_dir("elephc_cli_web_pruned_prelude");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--web")
        .arg(&php_path)
        .output()
        .expect("failed to compile pruned web program");
    assert!(
        output.status.success(),
        "elephc --web failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read web assembly");
    assert!(
        asm.contains("_fn__u__u_elephc_u_session_u_start_u_core"),
        "plain web assembly must retain the auto-start session core"
    );
    assert!(
        !asm.contains(".globl _fn_session_u_start\n"),
        "plain web assembly must not emit the public option-heavy session_start wrapper"
    );
    assert!(
        !asm.contains("_fn_session_u_set_u_save_u_handler"),
        "plain web assembly must not emit session_set_save_handler"
    );
    assert!(
        !asm.contains("__ElephcCallableSessionHandler"),
        "plain web assembly must not emit legacy callable-handler dispatch"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compile-time web isolation selects one bridge symbol and leaves the default
/// assembly byte-identical to an explicit `worker` selection.
#[test]
fn test_cli_web_isolation_selects_entry_symbol_at_compile_time() {
    let dir = make_cli_test_dir("elephc_cli_web_isolation_symbols");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let compile = |flags: &[&str]| {
        let output = elephc_cli_command(&dir)
            .args(flags)
            .arg(&php_path)
            .output()
            .expect("failed to compile web-isolation fixture");
        assert!(
            output.status.success(),
            "web-isolation compile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(dir.join("main.s")).expect("failed to read web-isolation assembly")
    };

    let default_worker = compile(&["--web"]);
    let explicit_worker = compile(&["--web", "--web-isolation=worker"]);
    assert_eq!(
        default_worker, explicit_worker,
        "plain --web must emit exactly the explicit worker entry path"
    );
    assert!(default_worker.contains("elephc_web_run"));
    assert!(!default_worker.contains("elephc_web_run_pool"));
    assert!(!default_worker.contains("elephc_web_run_request"));

    let pool = compile(&["--web", "--web-isolation=pool"]);
    assert!(pool.contains("elephc_web_run_pool"));
    assert!(!pool.contains("elephc_web_run_request"));

    let request = compile(&["--web", "--web-isolation=request"]);
    assert!(request.contains("elephc_web_run_request"));
    assert!(!request.contains("elephc_web_run_pool"));

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies web-isolation is rejected without web mode and reports invalid model names.
#[test]
fn test_cli_web_isolation_validation_errors_are_focused() {
    let dir = make_cli_test_dir("elephc_cli_web_isolation_errors");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let without_web = elephc_cli_command(&dir)
        .arg("--web-isolation=pool")
        .arg(&php_path)
        .output()
        .expect("failed to run web-isolation validation fixture");
    assert!(!without_web.status.success());
    assert!(
        String::from_utf8_lossy(&without_web.stderr).contains("--web-isolation requires --web"),
        "unexpected missing-web diagnostic: {}",
        String::from_utf8_lossy(&without_web.stderr)
    );

    let invalid = elephc_cli_command(&dir)
        .args(["--web", "--web-isolation=banana"])
        .arg(&php_path)
        .output()
        .expect("failed to run invalid web-isolation fixture");
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("expected worker|pool|request"),
        "unexpected invalid-mode diagnostic: {}",
        String::from_utf8_lossy(&invalid.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--with-pdo` roots the complete injected PDO group even without source-level PDO use.
#[test]
fn test_with_pdo_keeps_unreferenced_pdo_function() {
    let dir = make_cli_test_dir("elephc_cli_with_pdo_reachability");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-pdo")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile forced PDO assembly");
    assert!(
        output.status.success(),
        "elephc --with-pdo --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read PDO assembly");
    let symbol = elephc::names::function_symbol("pdo_drivers");
    assert!(
        asm.contains(&format!(".globl {symbol}\n")),
        "--with-pdo must keep unreferenced PDO declarations"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--with-crypto` force-links the bridge without force-injecting the hash prelude.
#[test]
fn test_with_crypto_does_not_force_hash_prelude() {
    let dir = make_cli_test_dir("elephc_cli_with_crypto_reachability");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-crypto")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile forced crypto assembly");
    assert!(
        output.status.success(),
        "elephc --with-crypto --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read crypto assembly");
    let hash_init = elephc::names::function_symbol("hash_init");
    assert!(
        !asm.contains(&format!(".globl {hash_init}\n")),
        "--with-crypto must not inject the source-level hash prelude"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--with-eval` keeps user declarations available to opaque runtime source.
#[test]
fn test_with_eval_keeps_unreferenced_user_declaration() {
    let dir = make_cli_test_dir("elephc_cli_with_eval_reachability");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php function runtime_only(): string { return 'eval'; } echo 'ok';",
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-eval")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile forced eval assembly");
    assert!(
        output.status.success(),
        "elephc --with-eval --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read eval assembly");
    let symbol = elephc::names::function_symbol("runtime_only");
    assert!(
        asm.contains(&format!(".globl {symbol}\n")),
        "--with-eval must keep unreferenced user declarations"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies repeated boxed-Mixed callable sites reuse module-wide descriptor
/// wrappers instead of regenerating the full candidate set in every function.
#[test]
fn test_cli_runtime_callable_descriptors_are_shared_across_call_sites() {
    let dir = make_cli_test_dir("elephc_cli_callable_descriptor_dedup");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class InvokableTarget { public function __invoke(int $value): int { return $value + 1; } }
function first(mixed $callback): mixed { return call_user_func($callback, 1); }
function second(mixed $callback): mixed { return call_user_func($callback, 2); }
function plus_one(int $value): int { return $value + 1; }
echo first('plus_one');
echo second('plus_one');
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile callable dedup fixture");
    assert!(
        output.status.success(),
        "callable dedup fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read callable assembly");
    assert!(
        asm.contains("_eir_first_callable_invoker"),
        "the first dynamic call site must emit shared invokers"
    );
    assert!(
        !asm.contains("_eir_second_callable_invoker"),
        "the second equivalent call site must reuse the first site's invokers"
    );

    let run = run_binary(&dir.join("main"), &dir);
    assert!(
        run.status.success(),
        "callable dedup fixture failed at runtime: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "23");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--emit-ir` prints textual EIR and stops before assembly, object,
/// or binary emission.
#[test]
fn test_cli_emit_ir_prints_eir_only() {
    let dir = make_cli_test_dir("elephc_cli_emit_ir");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function greet(): int {
    return 7;
}
echo greet();
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --emit-ir");

    assert!(
        output.status.success(),
        "elephc --emit-ir failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("module target="), "missing module header: {stdout}");
    assert!(stdout.contains("function greet"), "missing lowered function: {stdout}");
    assert!(stdout.contains("const_i64 7"), "missing lowered return literal: {stdout}");
    assert!(stdout.contains("function main"), "missing lowered main function: {stdout}");
    assert!(
        !dir.join("main.s").exists(),
        "--emit-ir should not emit assembly files"
    );
    assert!(
        !dir.join("main.o").exists(),
        "--emit-ir should not emit object files"
    );
    assert!(
        !dir.join("main").exists(),
        "--emit-ir should not emit binaries"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that passing `--emit-asm` and `--check` together fails with a
/// "mutually exclusive" error message.
#[test]
fn test_cli_rejects_emit_asm_and_check_together() {
    let dir = make_cli_test_dir("elephc_cli_flag_conflict");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg("--check")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with conflicting flags");

    assert!(
        !output.status.success(),
        "expected conflicting flags to fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("mutually exclusive"),
        "expected conflict message, got stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--emit-ir` participates in the same exclusive output-mode group
/// as `--emit-asm` and `--check`.
#[test]
fn test_cli_rejects_emit_ir_output_mode_conflicts() {
    let dir = make_cli_test_dir("elephc_cli_emit_ir_flag_conflict");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    for conflicting_flag in ["--emit-asm", "--check"] {
        let output = elephc_cli_command(&dir)
            .arg("--emit-ir")
            .arg(conflicting_flag)
            .arg(&php_path)
            .output()
            .expect("failed to run elephc CLI with conflicting --emit-ir flag");

        assert!(
            !output.status.success(),
            "expected --emit-ir {conflicting_flag} to fail"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("mutually exclusive"),
            "expected conflict message, got stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--check --timings` renders the frontend phase table without
/// reporting code generation, assembly, or linking phases.
#[test]
fn test_cli_timings_reports_check_phases() {
    let dir = make_cli_test_dir("elephc_cli_timings_check");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--check")
        .arg("--timings")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --timings --check");

    assert!(
        output.status.success(),
        "elephc --timings --check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Compiler timings"), "missing timings header: {stderr}");
    assert!(stderr.contains("Tokenizing source"), "missing tokenize timing: {stderr}");
    assert!(stderr.contains("Parsing program"), "missing parse timing: {stderr}");
    assert!(stderr.contains("Checking types"), "missing typecheck timing: {stderr}");
    assert!(stderr.contains("Total"), "missing total timing: {stderr}");
    assert!(
        !stderr.contains("Generating native code"),
        "unexpected codegen timing in --check output: {stderr}"
    );
    assert!(
        !stderr.contains("Assembling object file"),
        "unexpected assemble timing in --check output: {stderr}"
    );
    assert!(
        !stderr.contains("Linking native output"),
        "unexpected link timing in --check output: {stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--timings` renders the native build phases and total duration
/// when compiling a full binary, and that the binary is emitted.
#[test]
fn test_cli_timings_reports_assemble_and_link() {
    let dir = make_cli_test_dir("elephc_cli_timings_build");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--timings")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --timings");

    assert!(
        output.status.success(),
        "elephc --timings failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Generating native code"),
        "missing codegen timing: {stderr}"
    );
    assert!(
        stderr.contains("Assembling object file"),
        "missing assemble timing: {stderr}"
    );
    assert!(
        stderr.contains("Linking native output"),
        "missing link timing: {stderr}"
    );
    assert!(stderr.contains("Total"), "missing total timing: {stderr}");
    assert!(dir.join("main").exists(), "expected compiled binary to exist");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the timing report records `Runtime cache: miss` for the first
/// compile and `Runtime cache: hit` for the second without rebuilding it.
#[test]
fn test_cli_runtime_cache_reuses_runtime_object() {
    let dir = make_cli_test_dir("elephc_cli_runtime_cache");
    let cache_root = dir.join("cache-root");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let first = Command::new(elephc_cli_bin())
        .arg("--timings")
        .arg(&php_path)
        .env("XDG_CACHE_HOME", &cache_root)
        .current_dir(&dir)
        .output()
        .expect("failed to run first elephc CLI compile with runtime cache");
    assert!(
        first.status.success(),
        "first cached compile failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stderr = String::from_utf8_lossy(&first.stderr);
    assert!(
        first_stderr.contains("Notes"),
        "expected timing notes after first compile, got stderr={first_stderr}"
    );
    assert!(
        first_stderr.contains("Runtime cache: miss"),
        "expected first compile to miss runtime cache, got stderr={first_stderr}"
    );

    let cache_dir = cache_root.join("elephc");
    let cached_objects: Vec<_> = fs::read_dir(&cache_dir)
        .expect("expected runtime cache directory to exist")
        .map(|entry| entry.expect("cache entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("o"))
        .collect();
    assert_eq!(
        cached_objects.len(),
        1,
        "expected exactly one cached runtime object, got {:?}",
        cached_objects
    );

    let second = Command::new(elephc_cli_bin())
        .arg("--timings")
        .arg(&php_path)
        .env("XDG_CACHE_HOME", &cache_root)
        .current_dir(&dir)
        .output()
        .expect("failed to run second elephc CLI compile with runtime cache");
    assert!(
        second.status.success(),
        "second cached compile failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        second_stderr.contains("Notes"),
        "expected timing notes after second compile, got stderr={second_stderr}"
    );
    assert!(
        second_stderr.contains("Runtime cache: hit"),
        "expected second compile to hit runtime cache, got stderr={second_stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--source-map` emits a sidecar .map file in the v2 schema:
/// versioned envelope, function ranges (user function + main), labels, and
/// opcode-tagged line mappings.
#[test]
fn test_cli_source_map_writes_sidecar_file() {
    let dir = make_cli_test_dir("elephc_cli_source_map");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function foo(int $x): int {
    return $x + 1;
}
echo foo(1);
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg("--source-map")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --source-map");

    assert!(
        output.status.success(),
        "elephc --source-map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let map_path = dir.join("main.map");
    assert!(map_path.exists(), "expected source map sidecar to exist");
    let map = fs::read_to_string(&map_path).expect("failed to read source map");
    assert!(
        map.contains("\"format\": \"elephc-source-map\""),
        "missing source map format header: {map}"
    );
    assert!(
        map.contains("\"version\": 2"),
        "missing source map schema version: {map}"
    );
    assert!(
        map.contains("\"asm\":"),
        "expected source map to record the asm path: {map}"
    );
    assert!(
        map.contains("\"name\": \"foo\""),
        "expected a function entry for foo: {map}"
    );
    assert!(
        map.contains("\"name\": \"main\""),
        "expected a function entry for main: {map}"
    );
    assert!(
        map.contains("\"php_line\": 3"),
        "expected a mapping for the return on PHP line 3: {map}"
    );
    assert!(
        map.contains("\"op\": \""),
        "expected opcode-tagged mappings: {map}"
    );
    assert!(
        map.contains("\"labels\": ["),
        "expected a labels section: {map}"
    );
    assert!(
        map.contains("\"source_sha256\": \""),
        "expected a source checksum: {map}"
    );
    assert!(
        map.contains("\"synthetic\": true") && map.contains("\"synthetic\": false"),
        "expected both user and synthetic function entries: {map}"
    );
    assert!(
        map.contains("\"block\": \"entry\""),
        "expected an entry-block label annotation: {map}"
    );
    assert!(
        map.contains("\"php_end_col\":"),
        "expected expression end positions in mappings: {map}"
    );
    assert!(
        map.contains("\"lines\": ["),
        "expected the PHP-line inverse index: {map}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--debug-info` injects DWARF line-table directives into the emitted
/// assembly: one `.file 1` header and a `.loc 1 <line> <col>` per source marker.
#[test]
fn test_cli_debug_info_injects_dwarf_line_directives() {
    let dir = make_cli_test_dir("elephc_cli_debug_info");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
echo 1 + 2;
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg("--debug-info")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --debug-info");

    assert!(
        output.status.success(),
        "elephc --debug-info failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm_path = dir.join("main.s");
    let asm = fs::read_to_string(&asm_path).expect("failed to read assembly");
    assert!(
        asm.starts_with(".file 1 \""),
        "expected .file header at top of assembly, got: {}",
        &asm[..asm.len().min(120)]
    );
    assert!(
        asm.contains(".loc 1 2 "),
        "expected a .loc directive for PHP line 2: {asm}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--debug-info` survives a source path that carries assembler string
/// metacharacters. A `\` used to be spliced into `.file`/`.asciz` unescaped, so
/// the assembler rejected the module outright; combined with `"` it terminated
/// the directive string early and let the rest of the path be assembled as
/// directives. The full compile must now succeed and the program must run.
#[test]
fn test_cli_debug_info_escapes_metacharacters_in_source_path() {
    let dir = make_cli_test_dir("elephc_cli_debug_info_escapes");
    // A backslash alone broke the assembler; `\"` was the directive-injection
    // vector. Both are legal filename bytes on every supported target.
    let php_path = dir.join("bs\\la\"sh.php");
    fs::write(
        &php_path,
        r#"<?php
function greet(): void { echo "escaped\n"; }
greet();
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--debug-info")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --debug-info");

    assert!(
        output.status.success(),
        "elephc --debug-info failed for a path with `\\` and `\"`: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("bs\\la\"sh.s")).expect("failed to read assembly");
    let file_line = asm.lines().next().expect("assembly is empty");
    assert!(
        file_line.contains("bs\\\\la\\\"sh.php\""),
        "source path must be escaped inside the .file string: {file_line}"
    );
    assert!(
        asm.contains(".asciz \"") && asm.contains("bs\\\\la\\\"sh.php\""),
        "source path must be escaped inside the compile-unit DW_AT_name too"
    );
    for line in asm.lines() {
        assert!(
            !line.starts_with(".globl bs") && !line.trim_start().starts_with("sh.php"),
            "path bytes leaked out of their directive: {line}"
        );
    }

    let run = std::process::Command::new(dir.join("bs\\la\"sh"))
        .output()
        .expect("failed to run the compiled binary");
    assert!(run.status.success(), "compiled binary did not run");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "escaped\n");

    let _ = fs::remove_dir_all(&dir);
}

/// Lists the symbol names `nm` reports from a linked binary's symbol table.
/// A fully stripped executable yields an empty list: `nm` either prints
/// nothing, reports "no symbols", or exits non-zero depending on the platform,
/// and all three shapes collapse to "no names" here.
fn symbol_table_names(binary: &Path) -> Vec<String> {
    let output = Command::new("nm")
        .arg(binary)
        .output()
        .expect("failed to run nm on the compiled binary");
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.contains("no symbols"))
        .filter_map(|line| line.split_whitespace().last().map(str::to_string))
        .collect()
}

/// Verifies linked executables are stripped of their symbol table by default
/// and that `--keep-symbols` retains it (runtime helper names become visible).
#[test]
fn test_cli_executables_strip_symbols_by_default_and_keep_symbols_retains_them() {
    let dir = make_cli_test_dir("elephc_cli_strip");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1 + 2;").unwrap();

    let stripped_build = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to run elephc for the default (stripped) build");
    assert!(
        stripped_build.status.success(),
        "default build failed: {}",
        String::from_utf8_lossy(&stripped_build.stderr)
    );
    let stripped_names = symbol_table_names(&dir.join("main"));
    assert!(
        !stripped_names.iter().any(|name| name.contains("__rt_")),
        "default build must not keep runtime helper names in its symbol table: {:?}",
        stripped_names
    );

    let kept_build = elephc_cli_command(&dir)
        .arg("--keep-symbols")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc --keep-symbols");
    assert!(
        kept_build.status.success(),
        "--keep-symbols build failed: {}",
        String::from_utf8_lossy(&kept_build.stderr)
    );
    let kept_names = symbol_table_names(&dir.join("main"));
    assert!(
        kept_names.iter().any(|name| name.contains("__rt_")),
        "--keep-symbols must retain runtime helper names in the symbol table"
    );
    assert!(
        kept_names.len() > stripped_names.len(),
        "--keep-symbols must keep strictly more symbols ({}) than the stripped default ({})",
        kept_names.len(),
        stripped_names.len()
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Returns the `incl` percentage the exact profile reports for `name`.
///
/// The table prints one row per function as `<name> <bar> incl <n>%  self …`.
/// Parsing the number rather than matching a literal keeps the assertion about
/// the attribution instead of about the formatting.
#[cfg(test)]
fn exact_incl_percent(report: &str, name: &str) -> f64 {
    for line in report.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() != Some(name) {
            continue;
        }
        let Some(index) = line.find("incl") else {
            continue;
        };
        let after = &line[index + 4..];
        let Some(percent) = after.split('%').next() else {
            continue;
        };
        if let Ok(value) = percent.trim().parse::<f64>() {
            return value;
        }
    }
    panic!("the profile has no row for `{name}`:\n{report}");
}

/// A suspended coroutine is not charged for what its consumer does.
///
/// The only test on this branch that runs with the profiler ACTIVE. Every other
/// check of the attribution is either a unit test driving `State` directly — which
/// cannot see the emitted hooks — or the dormant-output test above, which asserts
/// the profiler stays quiet and so is blind to what it says when it speaks.
///
/// `yield` and `Fiber::suspend` switch stacks rather than returning, so the body's
/// frame used to stay open across everything the consumer did next: it became the
/// caller of that work and the owner of its cost. Measured before the fix, a
/// generator body that ran 23 µs reported **99.8%** of the program's inclusive
/// time, and the call graph carried an edge from it to a function the loop called
/// and the generator never did. A delegating generator reported 52.6% for the same
/// reason on the `yield from` half.
///
/// The thresholds are deliberately loose. The gap being asserted is three orders
/// of magnitude — a coroutine that runs microseconds against a consumer that runs
/// milliseconds — so anything under 10% passes and every shape of the defect
/// lands far above it.
#[test]
fn test_cli_monitor_does_not_charge_a_coroutine_for_its_consumer() {
    let dir = make_cli_test_dir("elephc_cli_monitor_coroutine");
    fs::write(
        dir.join("coro.php"),
        "<?php\n\
         function heavy(int $rounds): int { $n = 0; for ($i = 0; $i < $rounds; $i++) { $n += $i; } return $n; }\n\
         function inner(): iterable { yield 1; yield 2; }\n\
         function outer(): iterable { yield 0; yield from inner(); }\n\
         function body(): int { Fiber::suspend(1); return 7; }\n\
         function drain(): int {\n\
         $t = 0;\n\
         foreach (outer() as $v) { $t += heavy(120000); }\n\
         $f = new Fiber('body');\n\
         $f->start();\n\
         $t += heavy(120000);\n\
         $f->resume(0);\n\
         return $t + $f->getReturn();\n\
         }\n\
         echo drain();\n",
    )
    .expect("failed to write the coroutine fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--with-monitoring", "coro.php"])
        .output()
        .expect("failed to compile the coroutine fixture");
    assert!(
        compile.status.success(),
        "compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let watched = elephc_cli_command(&dir)
        .args(["monitor", "./coro", "--dot", "coro.dot"])
        .output()
        .expect("failed to run elephc monitor");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&watched.stdout),
        String::from_utf8_lossy(&watched.stderr)
    );

    assert!(
        exact_incl_percent(&report, "heavy") > 90.0,
        "the consumer's own work should dominate:\n{report}"
    );
    for coroutine in ["outer", "inner", "body"] {
        let share = exact_incl_percent(&report, coroutine);
        assert!(
            share < 10.0,
            "`{coroutine}` was charged {share}% of the program while suspended:\n{report}"
        );
    }

    // The edge is the other half: a suspended coroutine that keeps its frame is
    // read as the CALLER of whatever the consumer does next, so the graph gains
    // an edge that does not exist. `drain` is what calls `heavy`.
    let graph = fs::read_to_string(dir.join("coro.dot")).expect("monitor wrote no graph");
    let node_of = |name: &str| -> Option<String> {
        graph.lines().find_map(|line| {
            let (id, rest) = line.trim().split_once(" [label=\"")?;
            rest.starts_with(name).then(|| id.to_string())
        })
    };
    let heavy = node_of("heavy").expect("the graph has no `heavy` node");
    for coroutine in ["outer", "inner", "body"] {
        if let Some(node) = node_of(coroutine) {
            assert!(
                !graph.contains(&format!("{node} -> {heavy}")),
                "the graph says `{coroutine}` calls `heavy`, which it never does:\n{graph}"
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

/// A coroutine resumed with a pending exception gets its frame back before its
/// own handler runs.
///
/// `__rt_fiber_suspend` does not always return to the suspension site. Three
/// paths leave without returning — `Fiber::suspend()` outside a fiber, a live
/// `unserialize()`, and a `Fiber::throw()`/`Generator::throw()` delivered on
/// resume — and all three reach PHP handlers. The second half of the bracket at
/// the suspension site is skipped, so the activation stayed parked while its own
/// `catch` ran.
///
/// The edge is what says it: `body`'s handler calls `heavy`, so `body → heavy`
/// is the true edge. With the activation still parked, `heavy` entered on a
/// stack whose top was the CONSUMER, and the graph read `drive → heavy` — a call
/// `drive` never makes. Asserting an edge that must EXIST, where the sibling
/// test asserts ones that must not.
///
/// The third path is the one real async code hits; the first two are error
/// paths that reach the same helper and are covered by the same hook.
#[test]
fn test_cli_monitor_restores_a_coroutine_resumed_into_a_throw() {
    let dir = make_cli_test_dir("elephc_cli_monitor_fiber_throw");
    fs::write(
        dir.join("ft.php"),
        "<?php\n\
         function heavy(int $rounds): int { $n = 0; for ($i = 0; $i < $rounds; $i++) { $n += $i; } return $n; }\n\
         function body(): int {\n\
         $t = 0;\n\
         try { Fiber::suspend(1); } catch (RuntimeException $e) { $t += heavy(300000); }\n\
         return $t;\n\
         }\n\
         function drive(): int {\n\
         $f = new Fiber('body');\n\
         $f->start();\n\
         $f->throw(new RuntimeException('x'));\n\
         return $f->getReturn();\n\
         }\n\
         echo drive();\n",
    )
    .expect("failed to write the fiber-throw fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--with-monitoring", "ft.php"])
        .output()
        .expect("failed to compile the fiber-throw fixture");
    assert!(
        compile.status.success(),
        "compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let watched = elephc_cli_command(&dir)
        .args(["monitor", "./ft", "--dot", "ft.dot"])
        .output()
        .expect("failed to run elephc monitor");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&watched.stdout),
        String::from_utf8_lossy(&watched.stderr)
    );
    assert!(
        report.contains("44999850000"),
        "the program's own answer changed, so the hooks are not transparent:\n{report}"
    );

    let graph = fs::read_to_string(dir.join("ft.dot")).expect("monitor wrote no graph");
    let node_of = |name: &str| -> String {
        graph
            .lines()
            .find_map(|line| {
                let (id, rest) = line.trim().split_once(" [label=\"")?;
                rest.starts_with(name).then(|| id.to_string())
            })
            .unwrap_or_else(|| panic!("the graph has no `{name}` node:\n{graph}"))
    };
    let (body, drive, heavy) = (node_of("body"), node_of("drive"), node_of("heavy"));
    assert!(
        graph.contains(&format!("{body} -> {heavy}")),
        "the handler's work was not attributed to the coroutine that ran it:\n{graph}"
    );
    assert!(
        !graph.contains(&format!("{drive} -> {heavy}")),
        "the graph says `drive` calls `heavy`, which only holds while `body` is \
         still parked:\n{graph}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `--live` works on every platform, because it asks the target it launched.
///
/// Deliberately NOT gated on macOS, which is the whole point of the test. The
/// live view used to read its own child from the outside, through a tool that
/// ships on macOS alone, and was refused everywhere else — for a program this
/// process had started ITSELF and could simply have asked. It now hands the
/// child a socketpair, exactly as the exact path always has, and the probe
/// answers snapshots on it.
///
/// The fixture runs to a wall-clock DEADLINE rather than an iteration count. A
/// count is a bet on the machine: calibrated here it gave five windows, and on a
/// faster CI runner the program finished inside the second one, so the loop saw
/// the child exit before it could redraw and the test failed for a reason that
/// had nothing to do with what it measures. Six seconds against a one-second
/// window leaves room on any machine, in both directions.
///
/// Three things are asserted, and the third is the one a reader would not think
/// of. Windows advance, so the loop is really looping. A PHP function is named,
/// so the answer carries symbolised frames rather than an empty table anyone
/// could produce. And the program's own output survives while the profiler's
/// does not: asking wakes the EXACT profiler too, which writes its table to
/// stderr at exit, and forwarding that would print a raw dump under the live
/// view as if the program had written it.
#[test]
fn test_cli_monitor_live_needs_no_external_sampler() {
    let dir = make_cli_test_dir("elephc_cli_monitor_live");
    fs::write(
        dir.join("hot.php"),
        "<?php\n\
         function spin(int $rounds): int { $n = 0; for ($i = 0; $i < $rounds; $i++) { $n = ($n + $i) % 1000003; } return $n; }\n\
         function descend(int $depth, int $rounds): int {\n\
         if ($depth <= 0) { return spin($rounds); }\n\
         return descend($depth - 1, $rounds) % 1000003;\n\
         }\n\
         $t = 0;\n\
         $deadline = microtime(true) + 6.0;\n\
         while (microtime(true) < $deadline) { $t = ($t + descend(6, 400000)) % 1000003; }\n\
         echo 'done', $t;\n",
    )
    .expect("failed to write the live fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--with-monitoring", "hot.php"])
        .output()
        .expect("failed to compile the live fixture");
    assert!(
        compile.status.success(),
        "compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let watched = elephc_cli_command(&dir)
        .args(["monitor", "./hot", "--live", "--duration", "1"])
        .output()
        .expect("failed to run elephc monitor --live");
    let stdout = String::from_utf8_lossy(&watched.stdout).to_string();
    let stderr = String::from_utf8_lossy(&watched.stderr).to_string();
    let report = format!("{stdout}{stderr}");

    assert!(
        report.contains("window 2"),
        "the live loop produced no second window:\n{report}"
    );
    assert!(
        report.contains("descend"),
        "the live table named no PHP function, so the frames were not symbolised:\n{report}"
    );
    assert!(
        report.contains("done"),
        "the program's own output was swallowed:\n{report}"
    );
    for leaked in ["elephc-probe:", "elephc-instr:"] {
        assert!(
            !report.contains(leaked),
            "raw profiler output reached the operator under the live view ({leaked}):\n{report}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// `--live` on a `.php` SOURCE, which is the way a user actually reaches it.
///
/// The live view asks the program it launched, so the program has to carry the
/// probe — and the live path compiled the source with `--debug-info` instead of
/// `--with-monitoring`, producing a binary with nothing listening. `monitor`
/// then opened a channel, waited out every window for an ACK that could not
/// come, and reported an empty table for a program running perfectly well.
///
/// Nothing caught it because the test above compiles the fixture by hand first
/// and monitors the BINARY. That is a real path, but it is not the one the
/// documentation puts first, and between them they left the common case
/// uncovered. This runs `monitor hot.php --live` and nothing else.
#[test]
fn test_cli_monitor_live_compiles_the_source_with_the_probe() {
    let dir = make_cli_test_dir("elephc_cli_monitor_live_source");
    fs::write(
        dir.join("hot.php"),
        "<?php\n\
         function spin(int $rounds): int { $n = 0; for ($i = 0; $i < $rounds; $i++) { $n = ($n + $i) % 1000003; } return $n; }\n\
         function descend(int $depth, int $rounds): int {\n\
         if ($depth <= 0) { return spin($rounds); }\n\
         return descend($depth - 1, $rounds) % 1000003;\n\
         }\n\
         $t = 0;\n\
         $deadline = microtime(true) + 6.0;\n\
         while (microtime(true) < $deadline) { $t = ($t + descend(6, 400000)) % 1000003; }\n\
         echo 'done', $t;\n",
    )
    .expect("failed to write the live source fixture");

    // No `--with-monitoring` step. That is the point of the test.
    let watched = elephc_cli_command(&dir)
        .args(["monitor", "hot.php", "--live", "--duration", "1"])
        .output()
        .expect("failed to run elephc monitor hot.php --live");
    let stdout = String::from_utf8_lossy(&watched.stdout).to_string();
    let stderr = String::from_utf8_lossy(&watched.stderr).to_string();
    let report = format!("{stdout}{stderr}");

    assert!(
        report.contains("descend"),
        "the live table named no PHP function, so the source was compiled without the probe \
         and the child had nothing to answer with:\n{report}"
    );
    assert!(
        !report.contains("did not answer within the window"),
        "the child never answered, which is what a binary compiled without the probe does:\n{report}"
    );
    assert!(
        report.contains("done"),
        "the program's own output was swallowed:\n{report}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// End-to-end `--with-monitoring`: a binary that carries the tooling is silent
/// until asked, and reports fully when `monitor` asks.
///
/// Both halves matter. The silence is the property that makes the capability
/// safe to ship — a program that starts emitting profiler output on its own
/// stderr would be a surprise its author cannot explain. And the reporting is
/// what the capability is for. Asserting only one of them would let the other
/// break unnoticed.
///
/// macOS-only: the fixture is CPU-bound so SIGPROF samples are guaranteed.
#[cfg(target_os = "macos")]
#[test]
fn test_cli_probe_embeds_in_process_sampler() {
    let dir = make_cli_test_dir("elephc_cli_probe");
    fs::write(
        dir.join("burn.php"),
        "<?php\nfunction burn(int $depth): int { $n = 0; for ($i = 0; $i < 6000000; $i = $i + 1) { $n = ($n + $i) % 1000003; } if ($depth > 0) { $n = ($n + burn($depth - 1)) % 1000003; } return $n; }\necho burn(4);\n",
    )
    .expect("failed to write the probe fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--with-monitoring", "burn.php"])
        .output()
        .expect("failed to run elephc --probe");
    assert!(
        compile.status.success(),
        "probe compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    // Run on its own: capable, but nobody asked.
    let alone = std::process::Command::new(dir.join("burn"))
        .output()
        .expect("failed to run the monitored binary");
    assert!(alone.status.success(), "monitored binary did not run");
    assert_eq!(String::from_utf8_lossy(&alone.stdout), "855");
    let quiet = String::from_utf8_lossy(&alone.stderr);
    assert!(
        !quiet.contains("elephc-probe") && !quiet.contains("elephc-instr"),
        "a binary nobody asked must not announce a profiler: {quiet}"
    );

    // Run through `monitor`, which asks over the control channel.
    let watched = elephc_cli_command(&dir)
        .args(["monitor", "./burn"])
        .output()
        .expect("failed to run elephc monitor");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&watched.stdout),
        String::from_utf8_lossy(&watched.stderr)
    );
    assert!(
        report.contains("burn"),
        "the profile should name the PHP function, symbolized from the embedded table: {report}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `--heap-debug --rt-ctx` together: the heap-debug free-list validator was
/// ctx-routed with the rest of the heap family, but a pattern test only pins
/// its emitted text. This drives a real alloc/free workload through the
/// validating allocator under ctx addressing on the host target — the
/// validator reads the per-context bins and free list through x28/r14, so a
/// misrouted validator reports corruption or misses real corruption here.
#[test]
fn test_cli_rt_ctx_heap_debug_validator_passes_clean_workload() {
    let dir = make_cli_test_dir("elephc_cli_rt_ctx_heap_debug");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\n$sum = 0;\nfor ($i = 0; $i < 5000; $i++) {\n    $a = [$i, $i + 1, $i + 2];\n    $s = 'v' . $i;\n    unset($a);\n    $sum = ($sum + strlen($s)) % 1000003;\n}\necho $sum;\n",
    )
    .expect("failed to write the rt-ctx heap-debug fixture");

    let output = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(&php_path)
        .output()
        .expect("failed to compile rt-ctx heap-debug fixture");
    assert!(
        output.status.success(),
        "elephc --rt-ctx --heap-debug compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bin = dir.join("main");
    let run = std::process::Command::new(&bin)
        .output()
        .expect("failed to run the rt-ctx heap-debug binary");
    assert!(
        run.status.success(),
        "rt-ctx heap-debug binary failed (validator corruption?): {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let ctx_out = String::from_utf8_lossy(&run.stdout);
    assert!(!ctx_out.trim().is_empty(), "heap-debug program printed nothing");

    let _ = fs::remove_dir_all(&dir);
}
