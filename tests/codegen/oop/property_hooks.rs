//! Purpose:
//! End-to-end codegen tests for PHP 8.4 property hooks (`public T $p { get => ...; set { ... } }`).
//! Hook bodies compile to synthetic accessor methods; external reads/writes route to them.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers virtual (computed) get, get+set with an explicit backing field, the backed pattern that
//!   references the property itself (raw-slot access inside the accessor), constructor writes, and
//!   inheritance of hooks by subclasses.

use super::*;

/// Verifies a virtual get-only hook computes its value from other properties on read.
#[test]
fn test_get_only_virtual_property() {
    let out = compile_and_run(
        "<?php
        class Person {
            public string $first = \"Ada\";
            public string $last = \"Lovelace\";
            public string $full {
                get => $this->first . \" \" . $this->last;
            }
        }
        $p = new Person();
        echo $p->full;
        ",
    );
    assert_eq!(out, "Ada Lovelace");
}

/// Verifies a get hook is invoked when the property is read from inside another method (`$this->p`).
#[test]
fn test_get_hook_via_this_in_method() {
    let out = compile_and_run(
        "<?php
        class Person {
            public string $first = \"Grace\";
            public string $last = \"Hopper\";
            public string $full {
                get => $this->first . \" \" . $this->last;
            }
            public function greet(): string { return \"Hi, \" . $this->full; }
        }
        echo (new Person())->greet();
        ",
    );
    assert_eq!(out, "Hi, Grace Hopper");
}

/// Verifies a get hook with a block body (`get { ... return ...; }`) works like the short form.
#[test]
fn test_get_hook_block_body() {
    let out = compile_and_run(
        "<?php
        class Rect {
            public int $w = 4;
            public int $h = 5;
            public int $area {
                get { return $this->w * $this->h; }
            }
        }
        echo (new Rect())->area;
        ",
    );
    assert_eq!(out, "20");
}

/// Verifies a get+set pair over an explicit backing field routes both directions correctly,
/// including a derived property whose set converts into the backing field.
#[test]
fn test_get_and_set_with_backing_field() {
    let out = compile_and_run(
        "<?php
        class Temperature {
            private float $c = 0.0;
            public float $celsius {
                get => $this->c;
                set { $this->c = $value; }
            }
            public float $fahrenheit {
                get => $this->c * 9.0 / 5.0 + 32.0;
                set { $this->c = ($value - 32.0) * 5.0 / 9.0; }
            }
        }
        $t = new Temperature();
        $t->celsius = 25.0;
        $f = $t->fahrenheit;
        $t->fahrenheit = 212.0;
        echo $t->celsius, \" \", $f;
        ",
    );
    assert_eq!(out, "100 77");
}

/// Verifies the backed pattern: a hook referencing the property itself reads/writes the raw backing
/// slot (the recursion guard), so a `set` can normalize the stored value.
#[test]
fn test_backed_property_set_normalizes() {
    let out = compile_and_run(
        "<?php
        class Name {
            public string $value {
                get => $this->value;
                set { $this->value = trim($value); }
            }
        }
        $n = new Name();
        $n->value = \"  Ada  \";
        echo \"[\", $n->value, \"]\";
        ",
    );
    assert_eq!(out, "[Ada]");
}

/// Verifies a custom set-hook parameter name (`set(string $v)`) is honored in the body.
#[test]
fn test_set_hook_custom_parameter_name() {
    let out = compile_and_run(
        "<?php
        class Label {
            private string $s = \"\";
            public string $text {
                get => $this->s;
                set(string $v) { $this->s = strtoupper($v); }
            }
        }
        $l = new Label();
        $l->text = \"hi\";
        echo $l->text;
        ",
    );
    assert_eq!(out, "HI");
}

/// Verifies writing a hooked property in the constructor routes through the set hook.
#[test]
fn test_set_hook_from_constructor() {
    let out = compile_and_run(
        "<?php
        class User {
            private string $n = \"\";
            public string $name {
                get => $this->n;
                set { $this->n = ucfirst($value); }
            }
            public function __construct(string $name) { $this->name = $name; }
        }
        echo (new User(\"alice\"))->name;
        ",
    );
    assert_eq!(out, "Alice");
}

/// Verifies a subclass inherits its parent's hooked property and both directions still route.
#[test]
fn test_hooks_inherited_by_subclass() {
    let out = compile_and_run(
        "<?php
        class Base {
            private string $n = \"\";
            public string $name {
                get => $this->n;
                set { $this->n = ucfirst($value); }
            }
        }
        class Loud extends Base {
            public function shout(): string { return strtoupper($this->name); }
        }
        $l = new Loud();
        $l->name = \"bob\";
        echo $l->name, \" \", $l->shout();
        ",
    );
    assert_eq!(out, "Bob BOB");
}

/// Verifies a nullsafe read of a get-hooked property runs the hook on a non-null receiver while
/// short-circuiting to null otherwise.
#[test]
fn test_nullsafe_read_routes_to_get_hook() {
    let out = compile_and_run(
        "<?php
        class Person {
            public string $first = \"Ada\";
            public string $last = \"Lovelace\";
            public string $full {
                get => $this->first . \" \" . $this->last;
            }
        }
        function describe(?Person $p): string {
            return $p?->full ?? \"(none)\";
        }
        echo describe(new Person()), \"|\", describe(null);
        ",
    );
    assert_eq!(out, "Ada Lovelace|(none)");
}

/// Compiles and runs the checked-in `examples/property-hooks/main.php` fixture.
#[test]
fn test_example_property_hooks_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/property-hooks/main.php"));
    assert_eq!(out, "Jane Doe\nJANE DOE\ncelsius=100, fahrenheit=212\n");
}

/// Verifies a get-only hooked property whose name has uppercase letters: reading it works without
/// the recursion guard spuriously flagging the backing-slot write as a write to a read-only hooked
/// property. Regression test for case-insensitive accessor-name matching.
#[test]
fn test_mixed_case_get_only_hooked_property() {
    let out = compile_and_run(
        "<?php
        class C {
            private int $store = 0;
            public int $Total { get { return $this->store; } }
            public function set(int $v): void { $this->store = $v; }
        }
        $c = new C();
        $c->set(5);
        echo $c->Total;
        ",
    );
    assert_eq!(out, "5");
}
