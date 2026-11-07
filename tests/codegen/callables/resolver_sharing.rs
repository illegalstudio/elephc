//! Purpose:
//! Pins the shared resolver of boxed callables: what a site emits must not grow with the program.
//!
//! Called from:
//! - The codegen integration harness through the callables module.
//!
//! Key details:
//! - Resolving a `mixed` callable searches strings, callable arrays and invokable objects. Copied
//!   at every site, that sequence was a quarter of the code generation of a Symfony build.
//! - Counted on the assembly, so the bound holds on any machine and under any load.
//! - The resolver answers per class scope: a private method stays reachable from its own class
//!   only, which the behaviour tests below compare with php's output.

use crate::support::*;
use std::fs;

/// A class whose method resolves `count` boxed callables through a callable-typed parameter.
fn fixture(count: usize) -> String {
    let mut source = String::from(
        "<?php\nfunction take(callable $f): string { return $f('x'); }\n\
         function pick(array $pool, int $index): mixed { return $pool[$index]; }\n\
         class C {\n    public function run(array $pool, int $i): void {\n",
    );
    for _ in 0..count {
        source.push_str("        echo take(pick($pool, $i)), \"\\n\";\n");
    }
    source.push_str(
        "    }\n}\n(new C())->run(['strtoupper'], count($argv) - 1);\n",
    );
    source
}

/// Assembly emitted for `source`.
fn assembly(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_callable_resolver_sharing");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write the fixture");
    let output = elephc_cli_command(&dir)
        .args(["--emit-asm"])
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-asm");
    assert!(
        output.status.success(),
        "emitting assembly failed:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let assembly = fs::read_to_string(php_path.with_extension("s")).expect("read the assembly");
    let _ = fs::remove_dir_all(dir);
    assembly
}

/// Assembly lines emitted for `source`.
fn assembly_lines(source: &str) -> usize {
    assembly(source).lines().count()
}

/// Sixty more sites cost a call each, not one more search each.
#[test]
fn a_boxed_callable_site_costs_a_call_not_a_search() {
    let small = assembly_lines(&fixture(4));
    let large = assembly_lines(&fixture(64));
    let per_site = (large - small) / 60;
    assert!(per_site < 140, "each site added {per_site} assembly lines");
}

/// A class whose method calls `count` callable arrays built on the spot.
fn array_fixture(count: usize) -> String {
    let mut source = String::from(
        "<?php\nclass T { public function m(): int { return 1; } }\n\
         class C {\n    public function run($o, $m): int {\n        $n = 0;\n",
    );
    for _ in 0..count {
        source.push_str("        $n += [$o, $m]();\n");
    }
    source.push_str(
        "        return $n;\n    }\n}\necho (new C())->run(new T(), $argc > 1 ? 'x' : 'm'), \"\\n\";\n",
    );
    source
}

/// Every callable array site of a scope calls its one search.
#[test]
fn callable_array_sites_call_one_search() {
    let text = assembly(&array_fixture(64));
    let definitions = text
        .lines()
        .filter(|line| line.starts_with("_eir_shared_callable_array_lookup_"))
        .count();
    let calls = text
        .lines()
        .filter(|line| line.contains("_eir_shared_callable_array_lookup_") && !line.ends_with(':'))
        .filter(|line| line.trim_start().starts_with("bl ") || line.trim_start().starts_with("call "))
        .collect::<Vec<_>>();
    assert!(calls.len() >= 64, "only {} sites call the search", calls.len());
    let unique = calls.iter().collect::<std::collections::HashSet<_>>().len();
    assert!(definitions < 8 && unique < 8, "{definitions} searches for {unique} call targets");
}

/// One resolver per scope reports php's refusal from the right class, and each scope reaches its
/// own private method.
#[test]
fn a_shared_resolver_keeps_each_scope_reach() {
    let output = compile_and_run(
        r#"<?php
function pick(array $pool, int $index): mixed { return $pool[$index]; }
class A {
    private function secret(): string { return "A::secret"; }
    public function own(): string { $f = pick([[$this, 'secret']], 0); return $f(); }
    public function other(B $b): string {
        $f = pick([[$b, 'hidden']], 0);
        try { return $f(); } catch (\Error $e) { return get_class($e) . ": " . $e->getMessage(); }
    }
}
class B {
    private function hidden(): string { return "B::hidden"; }
    public function own(): string { $f = pick([[$this, 'hidden']], 0); return $f(); }
}
echo (new A())->own(), "\n";
echo (new B())->own(), "\n";
echo (new A())->other(new B()), "\n";
$f = pick([[new A(), 'secret']], 0);
try { $f(); } catch (\Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        output,
        "A::secret\nB::hidden\nError: Call to private method B::hidden() from scope A\n\
         Error: Call to private method A::secret() from global scope\n"
    );
}

/// The owned flag travels back from the shared body: an invokable object resolved twice frees its
/// bound descriptor each time, and an already resolved closure is not freed by the site.
#[test]
fn a_shared_resolver_hands_ownership_back_to_the_site() {
    let output = compile_and_run(
        r#"<?php
function pick(array $pool, int $index): mixed { return $pool[$index]; }
function take(callable $f): string { return $f("x"); }
class I { public function __invoke(string $s): string { return "i:" . $s; } }
class M { public function m(string $s): string { return "m:" . $s; } }
$pool = [new I(), [new M(), 'm'], fn(string $s): string => "c:" . $s, 'strtoupper'];
for ($round = 0; $round < 2; $round++) {
    foreach ([0, 1, 2, 3] as $i) {
        echo take(pick($pool, $i)), "\n";
    }
}
"#,
    );
    assert_eq!(output, "i:x\nm:x\nc:x\nX\ni:x\nm:x\nc:x\nX\n");
}
