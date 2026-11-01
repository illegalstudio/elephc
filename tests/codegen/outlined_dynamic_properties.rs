//! Purpose:
//! Regression tests for runtime-name property access (`$o->{$name}` read, isset, write, unset)
//! through the shared module-wide helpers instead of an inline ladder at every site.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The first group runs the access shapes under `--heap-debug` and compares with the output php
//!   prints, so a helper that changed visibility, diagnostics, coercion or ownership fails here.
//! - The last group inspects the emitted assembly: one definition of a helper, many calls to it.

use crate::support::*;
use std::fs;

/// Asserts a heap-debug run printed `expected` and ended with a clean leak summary.
fn assert_ran_clean(out: &ProgramOutput, expected: &str) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Declared public and non-public slots probed, read, written and removed through untyped
/// receivers from outside the class and from inside it, with a clean leak summary.
#[test]
fn test_runtime_name_access_on_untyped_receivers_respects_scope() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box {
    public $open = null;
    protected $guarded = null;
    private $hidden = null;
    public int $count = 5;
    function read($t, $n) { return $t->{$n}; }
    function probe($t, $n) { return isset($t->{$n}); }
    function bump($t) {
        foreach (["open", "guarded", "hidden", "count"] as $n) {
            $t->{$n} = 7;
        }
    }
    function clear($t) {
        foreach (["open", "hidden"] as $n) {
            unset($t->{$n});
        }
    }
}
function read($t, $n) { return $t->{$n}; }
function probe($t, $n) { return isset($t->{$n}); }
function bump($t) {
    foreach (["open", "guarded", "hidden", "count"] as $n) {
        try {
            $t->{$n} = 3;
            echo "wrote ", $n, "\n";
        } catch (Error $e) {
            echo get_class($e), ": ", $e->getMessage(), "\n";
        }
    }
}
function badCount($t) {
    $n = "count";
    try {
        $t->{$n} = "abc";
    } catch (TypeError $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
function clear($t) {
    foreach (["open", "hidden"] as $n) {
        try {
            unset($t->{$n});
            echo "unset ", $n, "\n";
        } catch (Error $e) {
            echo get_class($e), ": ", $e->getMessage(), "\n";
        }
    }
}
function run() {
    $a = new Box;
    $b = new Box;
    foreach (["open", "guarded", "hidden", "count"] as $n) {
        echo $n, " isset out=", var_export(probe($a, $n), true), " in=", var_export($b->probe($a, $n), true), "\n";
    }
    echo var_export(read($a, "open"), true), var_export(read($a, "count"), true), var_export($b->read($a, "guarded"), true), "\n";
    bump($a);
    $b->bump($a);
    badCount($a);
    echo $a->open, $a->count, $b->read($a, "hidden"), "\n";
    clear($a);
    $b->clear($a);
    echo var_export(probe($a, "open"), true), var_export($b->probe($a, "hidden"), true), "\n";
}
run();
run();
"#,
    );
    assert_ran_clean(
        &out,
        concat!(
            "open isset out=false in=false\n",
            "guarded isset out=false in=false\n",
            "hidden isset out=false in=false\n",
            "count isset out=true in=true\n",
            "NULL5NULL\n",
            "wrote open\n",
            "Error: Cannot access protected property Box::$guarded\n",
            "Error: Cannot access private property Box::$hidden\n",
            "wrote count\n",
            "TypeError: Cannot assign string to property Box::$count of type int\n",
            "777\n",
            "unset open\n",
            "Error: Cannot access private property Box::$hidden\n",
            "falsefalse\n",
            "open isset out=false in=false\n",
            "guarded isset out=false in=false\n",
            "hidden isset out=false in=false\n",
            "count isset out=true in=true\n",
            "NULL5NULL\n",
            "wrote open\n",
            "Error: Cannot access protected property Box::$guarded\n",
            "Error: Cannot access private property Box::$hidden\n",
            "wrote count\n",
            "TypeError: Cannot assign string to property Box::$count of type int\n",
            "777\n",
            "unset open\n",
            "Error: Cannot access private property Box::$hidden\n",
            "falsefalse\n",
        ),
    );
}

/// `AllowDynamicProperties` classes, stdClass and non-object receivers keep their answers.
#[test]
fn test_runtime_name_access_on_dynamic_and_non_object_receivers() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
#[AllowDynamicProperties]
class Bag { public $known = 1; }
class Plain { public $p = 1; }
function read($t, $n) { return $t->{$n}; }
function probe($t, $n) { return isset($t->{$n}); }
function put($t) { $n = "extra"; $t->{$n} = 9; }
function drop($t, $n) { unset($t->{$n}); }
function run() {
    $bag = new Bag;
    put($bag);
    echo read($bag, "known"), read($bag, "extra"), var_export(probe($bag, "extra"), true), "\n";
    drop($bag, "extra");
    echo var_export(probe($bag, "extra"), true), "\n";
    $std = new stdClass;
    put($std);
    echo read($std, "extra"), var_export(probe($std, "extra"), true), "\n";
    drop($std, "extra");
    echo var_export(probe($std, "extra"), true), "\n";
    $plain = new Plain;
    echo var_export(probe($plain, "nope"), true), var_export(probe(5, "p"), true), var_export(probe(null, "p"), true), "\n";
    drop(null, "p");
    echo read($plain, "p"), "\n";
}
run();
run();
"#,
    );
    assert_ran_clean(
        &out,
        concat!(
            "19true\n",
            "false\n",
            "9true\n",
            "false\n",
            "falsefalsefalse\n",
            "1\n",
            "19true\n",
            "false\n",
            "9true\n",
            "false\n",
            "falsefalsefalse\n",
            "1\n",
        ),
    );
}

/// A value read of a name the scope may not reach raises php's catchable `Error`, from outside
/// the class and not from inside it.
#[test]
fn test_runtime_name_read_refusals_come_from_the_shared_ladder() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public $open = "o";
    protected $guarded = "g";
    private $hidden = "h";
    public int $count = 5;
    function read($t, $n) { return $t->{$n}; }
    function probe($t, $n) { return isset($t->{$n}); }
    function write($t, $n, $v) { $t->{$n} = $v; }
    function drop($t, $n) { unset($t->{$n}); }
}
function read($t, $n) { return $t->{$n}; }
function probe($t, $n) { return isset($t->{$n}); }
function write($t, $n, $v) { $t->{$n} = $v; }
function attempt($f) {
    try { return $f(); } catch (Error $e) { return get_class($e) . ": " . $e->getMessage(); }
}
$a = new Box;
$b = new Box;
foreach (["open", "guarded", "hidden", "count"] as $n) {
    echo $n, " out=", attempt(fn() => read($a, $n)), " in=", attempt(fn() => $b->read($a, $n)), "\n";
    echo $n, " isset out=", var_export(probe($a, $n), true), " in=", var_export($b->probe($a, $n), true), "\n";
}
write($a, "open", "O");
$b->write($a, "hidden", "H");
$b->write($a, "count", "7");
echo attempt(function() use ($a) { write($a, "hidden", 1); return "wrote"; }), "\n";
echo attempt(function() use ($a) { write($a, "count", "abc"); return "wrote"; }), "\n";
$b->drop($a, "open");
echo var_export(probe($a, "open"), true), "\n";
echo $b->read($a, "hidden"), " ", $b->read($a, "count"), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "open out=o in=o\n",
            "open isset out=true in=true\n",
            "guarded out=Error: Cannot access protected property Box::$guarded in=g\n",
            "guarded isset out=false in=true\n",
            "hidden out=Error: Cannot access private property Box::$hidden in=h\n",
            "hidden isset out=false in=true\n",
            "count out=5 in=5\n",
            "count isset out=true in=true\n",
            "Error: Cannot access private property Box::$hidden\n",
            "TypeError: Cannot assign string to property Box::$count of type int\n",
            "false\n",
            "H 7\n",
        )
    );
}

/// Compiles `source` with `--emit-asm` for `target` and returns the assembly text.
fn emit_asm_for(prefix: &str, source: &str, target: &str) -> String {
    let dir = make_cli_test_dir(prefix);
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();
    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg(target)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --emit-asm");
    assert!(
        output.status.success(),
        "elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read emitted assembly");
    let _ = fs::remove_dir_all(&dir);
    asm
}

/// Counts the assembly lines that define a label starting with `prefix`.
fn count_definitions(asm: &str, prefix: &str) -> usize {
    asm.lines()
        .filter(|line| line.starts_with(prefix) && line.trim_end().ends_with("_0:"))
        .count()
}

/// Counts the assembly lines that call a label starting with `prefix`.
fn count_calls(asm: &str, prefix: &str) -> usize {
    asm.lines()
        .filter(|line| {
            let line = line.trim();
            (line.starts_with("bl ") || line.starts_with("call ")) && line.contains(prefix)
        })
        .count()
}

const MANY_SITES: &str = r#"<?php
class One { public $a = 1; private $p = 2; function peek($t, $n) { return $t->{$n}; } function put($t, $n, $v) { $t->{$n} = $v; } function drop($t, $n) { unset($t->{$n}); } }
class Two { public $b = 1; private $q = 2; function peek($t, $n) { return $t->{$n}; } function put($t, $n, $v) { $t->{$n} = $v; } function drop($t, $n) { unset($t->{$n}); } }
class Three { public $c = 1; function peek($t, $n) { return $t->{$n}; } function put($t, $n, $v) { $t->{$n} = $v; } function drop($t, $n) { unset($t->{$n}); } }
function read1($t, $n) { return $t->{$n}; }
function read2($t, $n) { return $t->{$n}; }
function read3($t, $n) { return $t->{$n}; }
function write1($t, $n, $v) { $t->{$n} = $v; }
function write2($t, $n, $v) { $t->{$n} = $v; }
function drop1($t, $n) { unset($t->{$n}); }
function drop2($t, $n) { unset($t->{$n}); }
$o = new One;
echo read1($o, "a"), read2($o, "a"), read3($o, "a"), (new Two)->peek(new Three, "c"), "\n";
write1($o, "a", 2); write2($o, "a", 3); drop1($o, "a"); drop2($o, "zz");
(new Three)->put(new One, "a", 1); (new Three)->drop(new One, "a"); (new One)->put(new One, "p", 1);
"#;

/// Every global-scope site calls the one module-wide helper per access kind, and the helper
/// bodies exist once, on both targets.
#[test]
fn test_runtime_name_access_sites_share_one_helper_per_kind() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let asm = emit_asm_for("elephc_dyn_prop_shared", MANY_SITES, target);
        for (kind, min_calls) in [("dyn_prop_get", 3), ("dyn_prop_set", 3), ("dyn_prop_unset", 3)] {
            let prefix = format!("_eir_shared_{kind}_");
            let defs = count_definitions(&asm, &prefix);
            let calls = count_calls(&asm, &prefix);
            // One global helper, plus at most one thin scoped helper per class that resolves
            // some name differently from the global scope (here: One and Two).
            assert!(
                (1..=3).contains(&defs),
                "{target}: expected a handful of {kind} helpers, found {defs}"
            );
            assert!(
                calls >= min_calls + defs.saturating_sub(1),
                "{target}: expected the sites to call the {kind} helpers, found {calls} calls to {defs} helpers"
            );
        }
    }
}

/// The shared helpers must be dramatically smaller than the inline ladders they replace; a
/// site that regressed to the inline ladder would bring back a per-site copy of the class table.
#[test]
fn test_runtime_name_access_helpers_are_emitted_once_not_per_site() {
    let asm = emit_asm_for("elephc_dyn_prop_once", MANY_SITES, "macos-aarch64");
    // The inline read ladder labels every arm `mixed_dyn_prop_get_<property>` inside the
    // requesting function; the shared one defines its arms inside the helper only.
    let helper_arms = asm
        .lines()
        .filter(|line| line.contains("mixed_dyn_prop_get_a_") && line.trim_end().ends_with(':'))
        .count();
    assert_eq!(helper_arms, 1, "the `a` arm must exist exactly once");
}

/// A runtime name that arrives as a boxed `Mixed` is cast to a string for the write, the read and
/// the removal alike; the write used to leave that string (and a call-result receiver) behind
/// on every call, and a refused read caught by `catch` left it behind once per throw. Looped so a
/// per-iteration leak cannot hide as a value that is simply live at exit.
#[test]
fn test_runtime_name_write_and_refused_read_release_their_name_and_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box {
    public $open = "o";
    public $n = 1;
    private $hidden = null;
}
function make() { return new Box; }
function write($t, $n, $v) { $t->{$n} = $v; }
function writeTemp($n, $v) { make()->{$n} = $v; return 1; }
function read($t, $n) { return $t->{$n}; }
function once() {
    $a = new Box;
    write($a, "open", "O");
    write($a, "n", 5);
    $sum = writeTemp("open", "T") + writeTemp("n", 9);
    $out = read($a, "open") . read($a, "n") . $sum;
    foreach (["open", "hidden"] as $n) {
        try {
            $out .= var_export(read($a, $n), true);
        } catch (Error $e) {
            $out .= get_class($e);
        }
    }
    return $out;
}
for ($i = 0; $i < 25; $i++) {
    $r = once();
}
echo $r, "\n";
"#,
    );
    assert_ran_clean(&out, "O52'O'Error\n");
}
