//! Purpose:
//! Integration regressions for the SHARED weak-mode typed-property guard: one helper per
//! declared type serves every property of that type, so a write through a runtime name or a
//! `mixed` receiver must still name the right class and property, coerce and deprecate the way
//! php does, unwind through the helper's frame, leave a clean heap, and keep the guard out of
//! the call sites.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs are the reference PHP 8.5 outputs for the same programs.
//! - Deprecations are written to stderr by this harness, so they are asserted separately from
//!   stdout and carry the line of the assignment that triggered them.

use super::*;

/// Every (class, property) pair below shares the guard of its declared type: the refusal text is a
/// runtime argument of the helper, so it has to keep naming the pair the write actually targeted.
/// Covers int, float, string, bool, nullable, union and class-typed destinations, a numeric-string
/// coercion, a float-to-int deprecation, and a Stringable object stored into string slots.
#[test]
fn test_shared_typed_property_guard_names_each_class_and_property() {
    let out = compile_and_run_capture(
        r#"<?php
interface Shape {}
class Dep implements Shape {
    public function __toString(): string { return "dep"; }
}
class Other {}
class A {
    public int $n = 0;
    public float $f = 0.0;
    public string $s = "";
    public bool $b = false;
    public ?int $q = null;
    public int|string $u = 0;
    public ?Dep $d = null;
    public Shape|int $sh = 0;
}
class B {
    public int $n = 1;
    public float $f = 1.5;
    public string $s = "b";
    public ?Dep $d = null;
}
class C {
    public int $count = 2;
    public string $label = "c";
}

function pick(string $k): mixed {
    switch ($k) {
        case "int": return 7;
        case "numeric": return "12";
        case "lossy": return 3.7;
        case "words": return "abc";
        case "list": return [1, 2];
        case "null": return null;
        case "dep": return new Dep();
        case "other": return new Other();
        case "true": return true;
        default: return 1.0;
    }
}

function make(string $c): mixed {
    if ($c === "A") { return new A(); }
    if ($c === "B") { return new B(); }
    return new C();
}

// Runtime-name writes through a mixed receiver: every (class, property) pair below shares the
// guard of its declared type, so the messages must still name the right class and property.
$plan = [
    ["A", "n", ["int", "numeric", "lossy", "words", "list", "null", "dep", "true"]],
    ["B", "n", ["int", "numeric", "words", "other"]],
    ["C", "count", ["int", "lossy", "words", "null"]],
    ["A", "f", ["int", "numeric", "words", "list"]],
    ["B", "f", ["int", "words", "dep"]],
    ["A", "s", ["int", "dep", "list", "null", "other", "true"]],
    ["B", "s", ["int", "dep", "list"]],
    ["C", "label", ["int", "dep", "other"]],
    ["A", "b", ["int", "words", "list", "null", "dep"]],
    ["A", "q", ["int", "numeric", "lossy", "words", "null", "dep"]],
    ["A", "u", ["int", "words", "lossy", "null", "list", "dep"]],
    ["A", "d", ["dep", "null", "other", "int", "list"]],
    ["B", "d", ["dep", "other", "words"]],
    ["A", "sh", ["dep", "int", "numeric", "other", "null"]],
];
foreach ($plan as [$class, $name, $kinds]) {
    $o = make($class);
    foreach ($kinds as $kind) {
        try {
            $o->$name = pick($kind);
            $v = $o->$name;
            echo "$class::$name <- $kind = ", is_object($v) ? get_class($v) : var_export($v, true), "\n";
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
    }
}
echo "done\n";
"#,
    );
    assert_eq!(
        out.stdout,
        r#"A::n <- int = 7
A::n <- numeric = 12
A::n <- lossy = 3
Cannot assign string to property A::$n of type int
Cannot assign array to property A::$n of type int
Cannot assign null to property A::$n of type int
Cannot assign Dep to property A::$n of type int
A::n <- true = 1
B::n <- int = 7
B::n <- numeric = 12
Cannot assign string to property B::$n of type int
Cannot assign Other to property B::$n of type int
C::count <- int = 7
C::count <- lossy = 3
Cannot assign string to property C::$count of type int
Cannot assign null to property C::$count of type int
A::f <- int = 7.0
A::f <- numeric = 12.0
Cannot assign string to property A::$f of type float
Cannot assign array to property A::$f of type float
B::f <- int = 7.0
Cannot assign string to property B::$f of type float
Cannot assign Dep to property B::$f of type float
A::s <- int = '7'
A::s <- dep = 'dep'
Cannot assign array to property A::$s of type string
Cannot assign null to property A::$s of type string
Cannot assign Other to property A::$s of type string
A::s <- true = '1'
B::s <- int = '7'
B::s <- dep = 'dep'
Cannot assign array to property B::$s of type string
C::label <- int = '7'
C::label <- dep = 'dep'
Cannot assign Other to property C::$label of type string
A::b <- int = true
A::b <- words = true
Cannot assign array to property A::$b of type bool
Cannot assign null to property A::$b of type bool
Cannot assign Dep to property A::$b of type bool
A::q <- int = 7
A::q <- numeric = 12
A::q <- lossy = 3
Cannot assign string to property A::$q of type ?int
A::q <- null = NULL
Cannot assign Dep to property A::$q of type ?int
A::u <- int = 7
A::u <- words = 'abc'
A::u <- lossy = 3
Cannot assign null to property A::$u of type string|int
Cannot assign array to property A::$u of type string|int
A::u <- dep = 'dep'
A::d <- dep = Dep
A::d <- null = NULL
Cannot assign Other to property A::$d of type ?Dep
Cannot assign int to property A::$d of type ?Dep
Cannot assign array to property A::$d of type ?Dep
B::d <- dep = Dep
Cannot assign Other to property B::$d of type ?Dep
Cannot assign string to property B::$d of type ?Dep
A::sh <- dep = Dep
A::sh <- int = 7
A::sh <- numeric = 12
Cannot assign Other to property A::$sh of type Shape|int
Cannot assign null to property A::$sh of type Shape|int
done
"#
    );
    let deprecations = out
        .stderr
        .lines()
        .filter(|line| line.starts_with("Deprecated: Implicit conversion from float 3.7 to int"))
        .collect::<Vec<_>>();
    assert_eq!(deprecations.len(), 4, "{}", out.stderr);
    for line in deprecations {
        assert!(line.ends_with(" on line 71"), "{}", out.stderr);
    }
}

/// A refusal raised inside the shared helper unwinds through the helper's frame, the caller's
/// `finally` and into the caller's `catch`, every time, and the accepted and coerced arms leave
/// no box, string or object behind.
#[test]
fn test_shared_typed_property_guard_unwinds_and_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Label { public function __toString(): string { return "label"; } }
class Other {}
class Tally { public static int $finally = 0; }
class Item {
    public ?Label $l = null;
    public string $s = "";
    public int|string $u = 0;
    public float $f = 0.0;
    public ?Item $next = null;
    public array $a = [];
    public int $n = 0;
}
class Pair {
    public ?Label $l = null;
    public string $s = "";
    public float $f = 0.0;
    public int $n = 0;
}

function write(mixed $o, string $name, mixed $v): string {
    try {
        try {
            $o->$name = $v;
            return "ok";
        } finally {
            Tally::$finally++;
        }
    } catch (TypeError $e) {
        return "E";
    }
}

function values(int $i): array {
    return [new Label(), "12", 3.0, [$i], null, new Other(), new Item(), "word", $i, true];
}


$ok = 0;
$bad = 0;
for ($i = 0; $i < 25; $i++) {
    foreach ([[new Item(), ["l", "s", "u", "f", "next", "a", "n"]], [new Pair(), ["l", "s", "f", "n"]]] as [$o, $names]) {
        foreach ($names as $name) {
            foreach (values($i) as $v) {
                if (write($o, $name, $v) === "ok") { $ok++; } else { $bad++; }
            }
        }
    }
}
echo $ok, " ", $bad, " ", Tally::$finally, "\n";
$item = new Item();
write($item, "u", "9");
write($item, "s", new Label());
write($item, "n", "4");
echo $item->u, "|", $item->s, "|", $item->n, "\n";
"#,
    );
    assert_eq!(out.stdout, "1025 1725 2750\n9|label|4\n");
    assert!(
        out.stderr.contains("leak summary: clean"),
        "expected a clean heap, got:\n{}",
        out.stderr
    );
}

/// The guard is emitted once per declared TYPE, however many classes, properties and sites write
/// to it: growing the program with more classes of the same property types adds call sites and
/// no helper bodies.
#[test]
fn test_typed_property_guard_helper_count_follows_types_not_sites() {
    fn program(classes: usize) -> String {
        let mut source = String::from("<?php\nclass Dep {}\n");
        for index in 0..classes {
            source.push_str(&format!(
                "class K{index} {{ public int $i = 0; public ?string $s = null; public ?Dep $d = null; }}\n"
            ));
        }
        source.push_str("function pick(int $k): mixed { return $k === 0 ? 7 : ($k === 1 ? \"x\" : null); }\n");
        source.push_str("function make(int $k): mixed {\n");
        for index in 0..classes {
            source.push_str(&format!("    if ($k === {index}) {{ return new K{index}(); }}\n"));
        }
        source.push_str("    return null;\n}\n");
        source.push_str("foreach (['i', 's', 'd'] as $name) {\n");
        for index in 0..classes {
            source.push_str(&format!(
                "    $o = make({index}); try {{ $o->$name = pick({index} % 3); }} catch (TypeError $e) {{ }}\n"
            ));
        }
        source.push_str("}\necho \"ok\\n\";\n");
        source
    }
    let definitions = |asm: &str| {
        asm.lines()
            .filter(|line| line.starts_with("_eir_shared_typed_prop_guard_") && line.ends_with(':'))
            .count()
    };
    let references = |asm: &str| asm.matches("_eir_shared_typed_prop_guard_").count();

    let dir = make_cli_test_dir("elephc_typed_prop_guard_small");
    let (small, _runtime, _libraries) =
        compile_source_to_asm_with_options(&program(3), &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    let dir = make_cli_test_dir("elephc_typed_prop_guard_large");
    let (large, _runtime, _libraries) =
        compile_source_to_asm_with_options(&program(12), &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);

    assert!(definitions(&small) >= 3, "one helper per declared type");
    assert_eq!(
        definitions(&small),
        definitions(&large),
        "more classes of the same property types must not add helper bodies"
    );
    assert!(
        references(&large) > references(&small) + 12,
        "each extra site must call the shared helper"
    );
    // The object arm's class ladder exists once per helper, not once per site.
    assert_eq!(
        large
            .lines()
            .filter(|line| line.trim().ends_with(':') && line.contains("_typed_prop_6_object_"))
            .count(),
        definitions(&large),
        "the object arm belongs to the helpers alone"
    );
}
