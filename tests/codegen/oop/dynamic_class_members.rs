//! Purpose:
//! End-to-end codegen tests for dynamic-receiver class member reads: `$class::NAME`,
//! `$object::NAME`, `$class::$prop`, and the `$this::NAME` / `$this::$prop` late-static forms.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Non-`$this` receivers desugar to `__elephc_dynamic_class_constant` /
//!   `__elephc_dynamic_static_property`, expanded by `ir_lower::expr::dynamic_class_access` into a
//!   case-insensitive runtime class-name dispatch. Receivers come from function parameters or
//!   runtime-built strings so no pass can fold the class name away.

use super::*;

/// Verifies `$class::NAME` resolves through a string receiver, case-insensitively, with a
/// leading backslash, inside a namespace, and through an inherited constant.
#[test]
fn test_dynamic_class_constant_from_string_receiver() {
    let out = compile_and_run(
        r#"<?php
        namespace App;
        class Base { const VERSION = '1.2.3'; const N = 40; }
        class Child extends Base {}
        function read(string $class) { return $class::VERSION . ':' . ($class::N + 2); }
        echo read('App\Base'), ' ', read('app\child'), ' ', read('\App\Child');
        "#,
    );
    assert_eq!(out, "1.2.3:42 1.2.3:42 1.2.3:42");
}

/// Verifies `$object::NAME` uses the object's runtime class, so a subclass override wins —
/// the psysh `$shell::VERSION` shape inside an array literal.
#[test]
fn test_dynamic_class_constant_from_object_receiver_uses_runtime_class() {
    let out = compile_and_run(
        r#"<?php
        class Shell { const VERSION = 'v0.12'; }
        class SubShell extends Shell { const VERSION = 'v-sub'; }
        function info($shell) {
            return ['PsySH version' => $shell::VERSION, 'Shell class' => \get_class($shell)];
        }
        foreach ([new Shell(), new SubShell()] as $shell) {
            $info = info($shell);
            echo $info['PsySH version'], '/', $info['Shell class'], ' ';
        }
        "#,
    );
    assert_eq!(out, "v0.12/Shell v-sub/SubShell ");
}

/// Verifies a `mixed` receiver that is sometimes an object and sometimes a class name.
#[test]
fn test_dynamic_class_constant_from_mixed_receiver() {
    let out = compile_and_run(
        r#"<?php
        class A { const N = 10; }
        class B { const N = 20; }
        function pick(int $i): mixed { return [new A(), 'B', new B(), 'a'][$i]; }
        $sum = 0;
        for ($i = 0; $i < 4; $i++) { $x = pick($i); $sum += $x::N; }
        echo $sum;
        "#,
    );
    assert_eq!(out, "60");
}

/// Verifies interface constants and enum cases are reachable through a runtime class name.
#[test]
fn test_dynamic_class_constant_interface_constant_and_enum_case() {
    let out = compile_and_run(
        r#"<?php
        interface HasV { const V = 'iface'; }
        class Impl implements HasV {}
        enum Suit { case Hearts; }
        function v(string $c) { return $c::V; }
        function hearts(string $c) { return $c::Hearts; }
        echo v('HasV'), ' ', v('Impl'), ' ', hearts('Suit') === Suit::Hearts ? 'same' : 'diff';
        "#,
    );
    assert_eq!(out, "iface iface same");
}

/// Verifies `$class::$prop` reads live static storage, including storage inherited by a child
/// and updated through the declaring class after the first read.
#[test]
fn test_dynamic_static_property_read() {
    let out = compile_and_run(
        r#"<?php
        class Cfg { public static $name = 'cfg'; public static int $count = 7; }
        class SubCfg extends Cfg {}
        function name(string $c) { return $c::$name; }
        $o = new SubCfg();
        echo name('Cfg'), ' ', $o::$count * 2, ' ';
        Cfg::$name = 'changed';
        echo name('SubCfg');
        "#,
    );
    assert_eq!(out, "cfg 14 changed");
}

/// Verifies `$this::NAME` and `$this::$prop` behave as `static::NAME` / `static::$prop`.
#[test]
fn test_this_scoped_member_reads_use_late_static_binding() {
    let out = compile_and_run(
        r#"<?php
        class Res {
            const K = 'res';
            public static $sp = 'sp';
            public function k() { return $this::K . '/' . $this::$sp; }
        }
        class SubRes extends Res { const K = 'sub'; }
        echo (new Res())->k(), ' ', (new SubRes())->k();
        "#,
    );
    assert_eq!(out, "res/sp sub/sp");
}

/// Verifies a property-access receiver (`$obj->prop::NAME`, the Symfony/Laravel shapes).
#[test]
fn test_dynamic_class_member_through_property_receiver() {
    let out = compile_and_run(
        r#"<?php
        class Client { const VERSION = '8.1.0'; public static $forceWrapping = true; }
        class Holder { public $resource; public $typeName = 'Client'; }
        $h = new Holder();
        $h->resource = new Client();
        echo $h->typeName::VERSION, ' ', $h->resource::$forceWrapping ? 'wrap' : 'nowrap';
        "#,
    );
    assert_eq!(out, "8.1.0 wrap");
}

/// Verifies a class referenced only through a runtime-built name survives reachability pruning.
#[test]
fn test_dynamic_class_member_keeps_class_named_only_at_runtime() {
    let out = compile_and_run(
        r#"<?php
        class OnlyByName { const TAG = 'kept'; public static $hits = 3; }
        function cls(): string { return strtolower('ONLY') . 'ByName'; }
        $c = cls();
        echo $c::TAG, ' ', $c::$hits;
        "#,
    );
    assert_eq!(out, "kept 3");
}

/// Verifies an unmatched class name throws a catchable `Error` instead of reading garbage.
///
/// elephc limitation: for an unknown class, PHP reports `Class "Nope" not found`. elephc reports
/// the member form (`Undefined constant Nope::X`, `Access to undeclared static property
/// Nope::$p`) because telling the two apart needs a runtime lookup of every class name at each
/// site. This test pins elephc's current message, not PHP's.
#[test]
fn test_dynamic_class_member_unknown_class_throws_catchable_error_elephc_message() {
    let out = compile_and_run(
        r#"<?php
        class Foo { const X = 1; public static $p = 2; }
        function get(string $c) { try { return $c::X; } catch (\Error $e) { return 'E:' . $e->getMessage(); } }
        function prop(string $c) { try { return $c::$p; } catch (\Error $e) { return 'E:' . $e->getMessage(); } }
        echo get('Foo'), '|', get('Nope'), '|', prop('Foo'), '|', prop('Nope');
        "#,
    );
    assert_eq!(
        out,
        "1|E:Undefined constant Nope::X|2|E:Access to undeclared static property Nope::$p"
    );
}

/// Verifies `--strict-php` accepts `$class::NAME` (plain PHP) even though the parser desugars it
/// to a compiler-reserved `__elephc_*` primitive.
#[test]
fn test_dynamic_class_constant_is_accepted_by_strict_php() {
    let out = compile_cli_file_and_run_with_flags(
        r#"<?php
        class Shell { const VERSION = 'v1'; }
        function v($shell) { return $shell::VERSION; }
        echo v(new Shell()), v('Shell');
        "#,
        &["--strict-php"],
    );
    assert_eq!(out, "v1v1");
}

/// Regression: `$class::$p` under `??`, `isset()` and `empty()` is a silent probe, as in PHP:
/// an uninitialized typed property, an undeclared name and a property the scope cannot see
/// answer the default / `false` / `true` instead of raising, while a plain read still raises.
#[test]
fn test_dynamic_static_property_silent_probes() {
    let out = compile_and_run(
        r#"<?php
        class C { public static int $p; private static int $priv = 3; public static $n = null; public static int $r = 5; }
        $c = 'C';
        echo $c::$p ?? 7, $c::$nope ?? 'a', $c::$priv ?? 'b', $c::$r ?? 9, "\n";
        var_dump(isset($c::$p), isset($c::$nope), isset($c::$priv), isset($c::$n), isset($c::$r));
        var_dump(empty($c::$p), empty($c::$nope), empty($c::$r));
        $o = new C;
        echo $o::$p ?? 'd', "\n";
        try { echo $c::$p, "\n"; } catch (\Error $e) { echo $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "7ab5\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nd\nTyped static property C::$p must not be accessed before initialization\n"
    );
}

/// Regression: the silent probe also covers `$class::$p` as the root of a subscript chain under
/// `??`, `isset()` and `empty()` (`$c::$p['x'] ?? 'd'` with `$p` an uninitialized typed array);
/// a hidden root, a missing key and a deeper chain answer silently too, and no warning is
/// raised.
#[test]
fn test_dynamic_static_property_subscript_silent_probes() {
    let out = compile_and_run(
        r#"<?php
        set_error_handler(function ($no, $msg) { echo "[warn: $msg]"; return true; });
        class C {
            public static array $p;
            private static array $hidden = ['x' => 1];
            public static $u = ['k' => ['n' => 5]];
        }
        $c = 'C';
        echo $c::$p['x'] ?? 'default', $c::$p['x']['y'] ?? 'deep', "\n";
        var_dump(isset($c::$p['x']), empty($c::$p['x']), isset($c::$p['x']['y']));
        echo $c::$u['k']['n'] ?? 'no', $c::$u['k']['m'] ?? 'nm', $c::$hidden['x'] ?? 'hid', "\n";
        var_dump(isset($c::$u['k']['n']), isset($c::$hidden['x']), empty($c::$nope['a']));
        C::$p = ['x' => 'set'];
        echo $c::$p['x'] ?? 'default', "\n";
        "#,
    );
    assert_eq!(
        out,
        "defaultdeep\nbool(false)\nbool(true)\nbool(false)\n5nmhid\nbool(true)\nbool(false)\nbool(true)\nset\n"
    );
}
