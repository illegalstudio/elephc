//! Purpose:
//! Regression tests for descriptor invokers reading parameter names from a data table instead of
//! spelling them into every invoker body.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The behaviour test compares with the output php prints for named-argument containers handed
//!   to `call_user_func_array()`: reordered, defaults skipped, unknown names, collisions,
//!   positional-after-named, string-keyed variadics, by-reference parameters, methods, static
//!   methods, invokable objects, closures and first-class callables.
//! - The assembly test asserts that the number of invoker BODIES does not grow with the number of
//!   functions whose signatures differ only in parameter names, on both targets. Bodies that
//!   spelled names into their text were all distinct, one per function.

use crate::support::*;
use std::fs;

/// Named-argument containers bound by descriptor invokers, compared with php.
#[test]
fn test_named_arguments_through_descriptor_invokers_match_php() {
    let out = compile_and_run(
        r####"<?php
function plain(int $a, string $b = "B", int $c = 3) { return "$a|$b|$c"; }
function pair(int $first, int $second) { return $first * 10 + $second; }
function vari(int $x, ...$rest) { return $x . ":" . json_encode($rest); }
function onlyvari(...$all) { return json_encode($all); }
function byref(int &$counter, int $step = 1) { $counter += $step; return $counter; }
function noargs() { return "none"; }
function mixedp($m, ?string $s = null, array $arr = [1]) { return json_encode([$m, $s, $arr]); }
class K {
    public function m(int $one, int $two = 2) { return "m:$one:$two"; }
    public static function s(string $name, bool $flag = false) { return "s:$name:" . var_export($flag, true); }
    public function __invoke(int $v, int $w = 0) { return "inv:$v:$w"; }
}
function J(string $json) { return json_decode($json, true); }
function t(string $label, callable $f) {
    try { echo $label, ": ", $f(), "\n"; } catch (Throwable $e) { echo $label, ": ", get_class($e), ": ", $e->getMessage(), "\n"; }
}
t("positional", fn() => call_user_func_array('plain', J('[1,"x",9]')));
t("named", fn() => call_user_func_array('plain', J('{"a":1,"c":7}')));
t("reordered", fn() => call_user_func_array('plain', J('{"c":7,"a":2,"b":"z"}')));
t("default skipped", fn() => call_user_func_array('plain', J('{"a":5}')));
t("pos then named", fn() => call_user_func_array('plain', J('{"0":4,"c":8}')));
t("unknown", fn() => call_user_func_array('plain', J('{"a":1,"nope":2}')));
t("overwrite", fn() => call_user_func_array('plain', J('{"0":1,"a":2}')));
t("pos after named", fn() => call_user_func_array('plain', J('{"a":1,"1":5}')));
t("pair named", fn() => call_user_func_array('pair', J('{"second":2,"first":1}')));
t("variadic pos", fn() => call_user_func_array('vari', J('[1,2,3]')));
t("variadic named extra", fn() => call_user_func_array('vari', J('{"x":1,"extra":2,"more":3}')));
t("variadic mixed", fn() => call_user_func_array('vari', J('{"0":1,"1":2,"k":3}')));
t("onlyvari str", fn() => call_user_func_array('onlyvari', J('{"a":1,"b":2}')));
t("onlyvari pos", fn() => call_user_func_array('onlyvari', J('[1,2]')));
$n = 10;
t("byref", function () use (&$n) { $args = J('{"step":5}'); $args['counter'] = &$n; $r = call_user_func_array('byref', $args); return "$r/$n"; });
$m = 1;
t("byref default", function () use (&$m) { $args = []; $args[0] = &$m; $r = call_user_func_array('byref', $args); return "$r/$m"; });
t("noargs", fn() => call_user_func_array('noargs', J('[]')));
t("noargs named", fn() => call_user_func_array('noargs', J('{"x":1}')));
t("mixed named", fn() => call_user_func_array('mixedp', J('{"s":"str","m":7}')));
t("mixed array default", fn() => call_user_func_array('mixedp', J('{"m":[1,2]}')));
$k = new K;
t("method named", fn() => call_user_func_array([$k, 'm'], J('{"two":9,"one":8}')));
t("static named", fn() => call_user_func_array('K::s', J('{"flag":true,"name":"n"}')));
t("static array named", fn() => call_user_func_array([K::class, 's'], J('{"name":"q"}')));
t("invokable named", fn() => call_user_func_array($k, J('{"w":2,"v":1}')));
t("closure named", fn() => call_user_func_array(fn(int $left, int $right = 1) => $left - $right, J('{"right":1,"left":10}')));
t("first-class", function () { $f = plain(...); return call_user_func_array($f, J('{"b":"fc","a":1}')); });
t("method unknown", fn() => call_user_func_array([$k, 'm'], J('{"one":1,"zz":2}')));
t("method overwrite", fn() => call_user_func_array([$k, 'm'], J('{"0":1,"one":2}')));
function nop() { return "nop"; }
function mixedp2($m, ?string $s = null, float $f = 1.5, bool $b = false, array $arr = [7]) { return json_encode([$m, $s, $f, $b, $arr]); }
$r = 4;
t("ref entry by value", function () use (&$r) { $a = J('{"b":"rb"}'); $a['a'] = &$r; $x = call_user_func_array('plain', $a); return $x . "/" . $r; });
t("nop empty named", fn() => call_user_func_array('nop', J('{}')));
t("holes and defaults", fn() => call_user_func_array('mixedp2', J('{"b":true,"m":1}')));
t("float from int", fn() => call_user_func_array('mixedp2', J('{"f":2,"m":"x"}')));
t("all named", fn() => call_user_func_array('mixedp2', J('{"arr":[1],"b":false,"f":0.5,"s":"s","m":null}')));
"####,
    );
    assert_eq!(out, "positional: 1|x|9\nnamed: 1|B|7\nreordered: 2|z|7\ndefault skipped: 5|B|3\npos then named: 4|B|8\nunknown: unknown: Error: Unknown named parameter $nope\noverwrite: overwrite: Error: Named parameter $a overwrites previous argument\npos after named: pos after named: Error: Cannot use positional argument after named argument\npair named: 12\nvariadic pos: 1:[2,3]\nvariadic named extra: 1:{\"extra\":2,\"more\":3}\nvariadic mixed: 1:{\"0\":2,\"k\":3}\nonlyvari str: {\"a\":1,\"b\":2}\nonlyvari pos: [1,2]\nbyref: 15/15\nbyref default: 2/2\nnoargs: none\nnoargs named: noargs named: Error: Unknown named parameter $x\nmixed named: [7,\"str\",[1]]\nmixed array default: [[1,2],null,[1]]\nmethod named: m:8:9\nstatic named: s:n:true\nstatic array named: s:q:false\ninvokable named: inv:1:2\nclosure named: 9\nfirst-class: 1|fc|3\nmethod unknown: method unknown: Error: Unknown named parameter $zz\nmethod overwrite: method overwrite: Error: Named parameter $one overwrites previous argument\nref entry by value: 4|rb|3/4\nnop empty named: nop\nholes and defaults: [1,null,1.5,true,[7]]\nfloat from int: [\"x\",null,2,false,[7]]\nall named: [null,\"s\",0.5,false,[1]]\n");
}

/// Compiles `source` with `--emit-asm` for `target` and returns the assembly text.
fn emit_asm(prefix: &str, source: &str, target: &str) -> String {
    emit_asm_with_env(prefix, source, target, &[])
}

/// [`emit_asm`] with extra environment variables for the compiler.
fn emit_asm_with_env(prefix: &str, source: &str, target: &str, env: &[(&str, &str)]) -> String {
    let dir = make_cli_test_dir(prefix);
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command
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

/// `count` functions with one shape (int, string default, int default) and unique parameter names,
/// each reached through `call_user_func_array()`.
fn many_named_signatures(count: usize) -> String {
    let mut php = String::from("<?php\n");
    for i in 0..count {
        php.push_str(&format!(
            "function f{i}(int $a{i}, string $b{i} = 'x', int $c{i} = 3) {{ return $a{i} + strlen($b{i}) + $c{i}; }}\n"
        ));
    }
    php.push_str("$names = [");
    for i in 0..count {
        php.push_str(&format!("'f{i}', "));
    }
    php.push_str("];\n$t = 0;\nforeach ($names as $f) { $t += call_user_func_array($f, [1, 'zz']); }\necho $t, \"\\n\";\n");
    php
}

/// Instructions in the emitted text: indented lines that are neither comments nor directives.
fn instruction_count(asm: &str) -> usize {
    asm.lines()
        .filter(|line| line.starts_with("    "))
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';') && !line.starts_with('.'))
        .count()
}

/// Signatures differing only in parameter names share their invoker bodies, so neither the
/// number of bodies nor the instructions per added function grow with the name count.
#[test]
fn test_invoker_bodies_do_not_grow_with_parameter_names() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm("elephc_invoker_names_small", &many_named_signatures(20), target);
        let large = emit_asm("elephc_invoker_names_large", &many_named_signatures(80), target);
        let bodies = |asm: &str| {
            asm.lines().filter(|line| line.contains("; runtime callable invoker ")).count()
        };
        assert_eq!(
            bodies(&small),
            bodies(&large),
            "{target}: invoker bodies must not grow with the number of distinct parameter names"
        );
        let per_function = (instruction_count(&large) - instruction_count(&small)) / 60;
        assert!(
            per_function < 400,
            "{target}: {per_function} instructions per added function, expected the function body and a stub only"
        );
    }
}

/// The number of invoker bodies a build LOWERS does not grow with signatures that differ only in
/// parameter names: a repeat of a known shape writes its stub and jumps to the body without
/// lowering it again. On the Symfony demo 79 474 invokers were lowered for 8 956 distinct
/// bodies, each repeat lowered, hashed and thrown away. Counted with `ELEPHC_INVOKER_STATS`,
/// which prints one `INVSHAPE` line per lowering.
#[test]
fn test_invoker_lowerings_do_not_grow_with_parameter_names() {
    let lowerings = |count: usize| {
        let dir = make_cli_test_dir("elephc_invoker_memo");
        let php_path = dir.join("main.php");
        fs::write(&php_path, many_named_signatures(count)).unwrap();
        let output = elephc_cli_command(&dir)
            .env("ELEPHC_INVOKER_STATS", "1")
            .arg("--emit-asm")
            .arg(&php_path)
            .output()
            .expect("failed to run elephc CLI");
        let _ = fs::remove_dir_all(&dir);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stderr).lines().filter(|line| line.starts_with("INVSHAPE")).count()
    };
    let small = lowerings(20);
    let large = lowerings(80);
    assert!(small > 0, "no invoker was lowered");
    assert_eq!(small, large, "invoker lowerings grew with the number of same-shape functions");
}

/// Callables whose signatures differ only in parameter names, and callables that differ in
/// types, define each shared invoker body and the shared normalizer exactly once, serial pass
/// and parallel pass alike. A body defined twice is an assembler error ("symbol
/// `_eir_cbody_...` is already defined").
#[test]
fn test_shared_invoker_bodies_are_defined_once() {
    let source = r#"<?php
function p1(int $left, int $right = 1) { return $left + $right; }
function p2(int $first, int $second = 1) { return $first + $second; }
function p3(string $left, int $right = 1) { return strlen($left) + $right; }
class Box { public function m1(int $one, int $two = 2) { return $one + $two; }
            public function m2(int $uno, int $dos = 2) { return $uno + $dos; }
            public static function s1(string $name) { return $name; } }
$cbs = ['p1', 'p2', 'p3', [new Box, 'm1'], [new Box, 'm2'], 'Box::s1'];
$t = 0;
foreach ($cbs as $cb) { $t += (int) call_user_func_array($cb, [2, 3]); }
echo $t, "\n";
"#;
    // A program large enough to take the parallel pass without forcing it: the PDO prelude's
    // methods reach the shared bodies from workers AND from the module's tail passes, which run
    // on the compiling thread's emitter after the last de-duplication. That emitter never saw
    // the workers' bodies, so the tail defined `_eir_cbody_*` and the normalizer a second time.
    let pdo_source = r#"<?php
$msg = "";
try { $bad = new PDO("nosuchdriver:x"); } catch (\PDOException $e) { $msg .= "caught"; }
$db = new PDO("sqlite::memory:");
try { $db->query("THIS IS NOT SQL"); } catch (\PDOException $e) { $msg .= "," . $e->errorInfo[0]; }
echo $msg;
"#;
    for (name, source) in [("small", source), ("pdo", pdo_source)] {
    for target in ["macos-aarch64", "linux-x86_64"] {
        for jobs in ["1", "4"] {
            let asm = emit_asm_with_env(
                "elephc_invoker_once",
                source,
                target,
                &[("ELEPHC_CODEGEN_PARALLEL_MIN", "1"), ("ELEPHC_CODEGEN_JOBS", jobs)],
            );
            let mut seen = std::collections::HashSet::new();
            for line in asm.lines() {
                let Some(label) = line.strip_suffix(':') else { continue };
                if label.starts_with("_eir_cbody_") || label == "_eir_invoker_assoc_to_indexed" {
                    assert!(
                        seen.insert(label.to_string()),
                        "{name} {target} jobs={jobs}: {label} is defined twice"
                    );
                }
            }
            assert!(seen.contains("_eir_invoker_assoc_to_indexed"), "{name} {target}: normalizer missing");
            assert!(seen.len() > 1, "{name} {target}: expected shared invoker bodies");
        }
    }
    }
}

/// Out-of-line invokers requested by a deferred body must not outlive its rollback: that left
/// duplicate global invoker labels, and descriptor data they pointed at undefined, once the
/// serial pass re-emitted the body.
#[test]
fn test_deferred_body_out_of_line_invokers_are_not_duplicated_or_orphaned() {
    let source = r#"<?php
#[Attribute] class Marker { public function __construct(public string $v = "m") {} }
#[Marker("a")] class Target {}
class Listener {
    public function one(): string { return $this->attr(Target::class, Marker::class)?->v ?? "none"; }
    public function two(): string { return $this->attr(Listener::class, Marker::class)?->v ?? "none"; }
    private function attr(string $class, string $attribute): ?object {
        $rc = new \ReflectionClass($class);
        $attrs = $rc->getAttributes($attribute);
        return $attrs ? $attrs[0]->newInstance() : null;
    }
}
$l = new Listener();
echo $l->one(), "|", $l->two(), "\n";
"#;
    for jobs in ["1", "4"] {
        let asm = emit_asm_with_env(
            "elephc_ool_dup",
            source,
            "macos-aarch64",
            &[("ELEPHC_CODEGEN_PARALLEL_MIN", "1"), ("ELEPHC_CODEGEN_JOBS", jobs)],
        );
        let mut defined = std::collections::HashSet::new();
        for line in asm.lines() {
            let Some(label) = line.strip_suffix(':') else { continue };
            let global = !label.is_empty()
                && !label.starts_with('L')
                && !label.starts_with('.')
                && !label.starts_with(|c: char| c.is_ascii_digit())
                && !label.contains(char::is_whitespace);
            if global {
                assert!(defined.insert(label.to_string()), "jobs={jobs}: {label} is defined twice");
            }
        }
        for token in asm.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if token.starts_with("_data_h") || token.starts_with("_str_h") {
                assert!(defined.contains(token), "jobs={jobs}: {token} is referenced but undefined");
            }
        }
    }
}
