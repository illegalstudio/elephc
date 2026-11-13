//! Purpose:
//! Regression tests for method calls whose receiver or result is not a single static class:
//! dynamic return types stored in literals, `__call` from a mixed receiver, and the catchable
//! errors PHP raises for a non-object or a class that does not declare the method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The cases share one program, so class and function names are unique and every `Error` is
//!   caught. A known-class `__call` and a mixed `__call` use different classes: a known receiver
//!   rewrites that class's args parameter before the mixed call is lowered.

use super::*;

/// Locks the method-call follow-ups from issues #2170, #2171, #2172, #2174, #2175, #2176,
/// and #2203 in one compile.
#[test]
fn test_method_call_followup_bundle() {
    assert_bundle(&[
        BundleCase {
            name: "object_param_string_returns",
            source: r#"<?php
class ObjRetA { public function f(): string { return "a"; } }
class ObjRetB { public function f(): string { return "b"; } }
function obj_ret_go(object $o) { return $o->f(); }
echo obj_ret_go(new ObjRetA()), obj_ret_go(new ObjRetB());
"#,
            expected: "ab",
        },
        BundleCase {
            name: "magic_call_in_literal",
            source: r#"<?php
class MagicLit {
    public function __call($name, $args) { return "M:" . $name . ":" . $args[0]; }
}
$magic_lit = new MagicLit();
echo [$magic_lit->missing("z")][0];
"#,
            expected: "M:missing:z",
        },
        BundleCase {
            name: "magic_call_through_mixed",
            source: r#"<?php
class MagicEach {
    public function __call($name, $args) { return "E:" . $name . ":" . $args[0]; }
}
class MagicEachOther {
    public function __call($name, $args) { return "E:" . $name . ":" . $args[0]; }
}
function magic_take(mixed $row) { return $row->missing("z"); }
$magic_each = "";
foreach ([new MagicEach(), new MagicEachOther()] as $magic_row) {
    $magic_each .= $magic_row->missing("z") . ";";
}
echo $magic_each, magic_take(new MagicEach()), ";";
"#,
            expected: "E:missing:z;E:missing:z;E:missing:z;",
        },
        BundleCase {
            name: "clone_receiver_in_literal",
            source: r#"<?php
interface CloneComponent { public function render(): string; }
final class CloneHtml implements CloneComponent {
    public function __construct(private string $m) {}
    public function render(): string { return $this->m; }
}
interface CloneBlock { public function component(string $path): CloneComponent; }
final class CloneNav implements CloneBlock {
    public function component(string $path): CloneComponent { return new CloneHtml("<nav>" . $path); }
}
final class CloneFoot implements CloneBlock {
    public function component(string $path): CloneComponent { return new CloneHtml("<foot>" . $path); }
}
function render_clone(array $blocks): string {
    $out = "";
    foreach ($blocks as $block) {
        $list = [(clone $block)->component("/x")];
        $out .= $list[0]->render() . "|";
    }
    return $out;
}
echo render_clone([new CloneNav(), new CloneFoot()]);
"#,
            expected: "<nav>/x|<foot>/x|",
        },
        BundleCase {
            name: "void_and_null_literal_elements",
            source: r#"<?php
class VoidBox {
    public function v(): void {}
    public function n() { return null; }
}
function void_fn(): void {}
function null_fn() { return null; }
$void_box = new VoidBox();
$void_items = [$void_box->v(), $void_box->n(), void_fn(), null_fn()];
var_dump($void_items[0], $void_items[1], $void_items[2], $void_items[3]);
"#,
            expected: "NULL\nNULL\nNULL\nNULL\n",
        },
        BundleCase {
            name: "static_non_object_receiver",
            source: r#"<?php
$mc_int = 1;
$mc_false = false;
$mc_true = true;
$mc_str = "ab";
$mc_arr = [1];
$mc_null = null;
$mc_float = 1.5;
try { $mc_int->f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $mc_false->f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $mc_true->f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $mc_str->f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $mc_arr->f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $mc_null->f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { $mc_float->f(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
            expected: "\
Call to a member function f() on int
Call to a member function f() on false
Call to a member function f() on true
Call to a member function f() on string
Call to a member function f() on array
Call to a member function f() on null
Call to a member function f() on float
",
        },
        BundleCase {
            name: "mixed_non_object_receiver",
            source: r#"<?php
function mc_mixed_message(mixed $value): string {
    try { $value->f(); } catch (Error $e) { return $e->getMessage(); }
    return "none";
}
echo mc_mixed_message(1), "\n";
echo mc_mixed_message(false), "\n";
echo mc_mixed_message(true), "\n";
echo mc_mixed_message("ab"), "\n";
echo mc_mixed_message([1]), "\n";
echo mc_mixed_message(null), "\n";
echo mc_mixed_message(1.5), "\n";
"#,
            expected: "\
Call to a member function f() on int
Call to a member function f() on false
Call to a member function f() on true
Call to a member function f() on string
Call to a member function f() on array
Call to a member function f() on null
Call to a member function f() on float
",
        },
        BundleCase {
            name: "undefined_method_after_rebind",
            source: r#"<?php
interface RtPlugin { public function name(): string; }
class RtSold implements RtPlugin {
    public function name(): string { return "sold"; }
    public function count(): int { return 2; }
}
class RtOther implements RtPlugin {
    public function name(): string { return "other"; }
}
class RtBox { public function __construct(public RtPlugin $plugin) {} }
function rebind_rt(RtBox &$box): void { $box = new RtBox(new RtOther()); }
$rt_box = new RtBox(new RtSold());
if ($rt_box->plugin instanceof RtSold) {
    rebind_rt($rt_box);
    echo "after=", $rt_box->plugin->name(), "\n";
    try { $rt_box->plugin->count(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
}
"#,
            expected: "after=other\nCall to undefined method RtOther::count()\n",
        },
        BundleCase {
            name: "undefined_method_on_mixed",
            source: r#"<?php
class RtBare { public function name(): string { return "bare"; } }
function mc_missing(mixed $obj): string {
    try { $obj->missing(); } catch (Error $e) { return $e->getMessage(); }
    return "none";
}
echo mc_missing(new RtBare());
"#,
            expected: "Call to undefined method RtBare::missing()",
        },
        BundleCase {
            name: "shared_scalar_and_nullable_literals",
            source: r#"<?php
interface FollowInt { public function n(): int; }
class FollowIntA implements FollowInt { public function n(): int { return 1; } }
class FollowIntB implements FollowInt { public function n(): int { return 2; } }
interface FollowStr { public function s(): string; }
class FollowStrA implements FollowStr { public function s(): string { return "a"; } }
class FollowStrB implements FollowStr { public function s(): string { return "b"; } }
interface FollowMaybe { public function m(): ?int; }
class FollowMaybeA implements FollowMaybe { public function m(): ?int { return null; } }
class FollowMaybeB implements FollowMaybe { public function m(): ?int { return 4; } }
function follow_scalars(array $ints, array $strs, array $maybes): void {
    foreach ($ints as $row) { $list = [$row->n()]; echo $list[0]; }
    echo "|";
    foreach ($strs as $row) {
        $list = [$row->s()];
        $keyed = ["a" => $row->s()];
        $nested = [[$row->s()]];
        echo $list[0], $keyed["a"], $nested[0][0];
    }
    echo "|";
    foreach ($maybes as $row) { $list = [$row->m()]; var_dump($list[0]); }
}
follow_scalars(
    [new FollowIntA(), new FollowIntB()],
    [new FollowStrA(), new FollowStrB()],
    [new FollowMaybeA(), new FollowMaybeB()]
);
"#,
            expected: "12|aaabbb|NULL\nint(4)\n",
        },
        BundleCase {
            name: "disagreeing_and_chained_literals",
            source: r#"<?php
class FollowMixA { public function g(): int { return 7; } }
class FollowMixB { public function g(): string { return "q"; } }
interface FollowStep { public function b(): string; }
class FollowStepA implements FollowStep { public function b(): string { return "A"; } }
class FollowStepB implements FollowStep { public function b(): string { return "B"; } }
interface FollowChain { public function a(): FollowStep; }
class FollowChainA implements FollowChain { public function a(): FollowStep { return new FollowStepA(); } }
class FollowChainB implements FollowChain { public function a(): FollowStep { return new FollowStepB(); } }
function follow_take(mixed $row): string { $list = [$row->s()]; return $list[0]; }
function follow_shapes(array $mixed, array $chains): void {
    foreach ($mixed as $row) { $list = [$row->g()]; echo gettype($list[0]), ";"; }
    echo "|";
    foreach ($chains as $row) { $list = [$row->a()->b()]; echo $list[0]; }
    echo "|", follow_take(new FollowStrA()), follow_take(new FollowStrB());
}
follow_shapes([new FollowMixA(), new FollowMixB()], [new FollowChainA(), new FollowChainB()]);
"#,
            expected: "integer;string;|AB|ab",
        },
        BundleCase {
            name: "float_bool_and_nullable_object_literals",
            source: r#"<?php
interface FollowFloat { public function flt(): float; }
class FollowFloatA implements FollowFloat { public function flt(): float { return 1.5; } }
class FollowFloatB implements FollowFloat { public function flt(): float { return 2.5; } }
interface FollowBool { public function flag(): bool; }
class FollowBoolA implements FollowBool { public function flag(): bool { return true; } }
class FollowBoolB implements FollowBool { public function flag(): bool { return false; } }
class FollowHtml { public function markup(): string { return "h"; } }
interface FollowMaybeObj { public function page(): ?FollowHtml; }
class FollowMaybeObjA implements FollowMaybeObj { public function page(): ?FollowHtml { return null; } }
class FollowMaybeObjB implements FollowMaybeObj { public function page(): ?FollowHtml { return new FollowHtml(); } }
function follow_more(array $floats, array $bools, array $objs): void {
    foreach ($floats as $row) { $flt_list = [$row->flt()]; var_dump($flt_list[0]); }
    foreach ($bools as $row) { $flag_list = [$row->flag()]; var_dump($flag_list[0]); }
    foreach ($objs as $row) {
        $page_list = [$row->page()];
        echo $page_list[0] === null ? "null" : $page_list[0]->markup();
    }
}
follow_more(
    [new FollowFloatA(), new FollowFloatB()],
    [new FollowBoolA(), new FollowBoolB()],
    [new FollowMaybeObjA(), new FollowMaybeObjB()]
);
"#,
            expected: "float(1.5)\nfloat(2.5)\nbool(true)\nbool(false)\nnullh",
        },
        BundleCase {
            name: "byref_return_in_literal",
            source: r#"<?php
class FollowRefA { public int $n = 5; public function &ref(): int { return $this->n; } }
class FollowRefB { public int $n = 6; public function &ref(): int { return $this->n; } }
function follow_ref(array $rows): void {
    foreach ($rows as $row) { $list = [$row->ref()]; echo $list[0]; }
}
follow_ref([new FollowRefA(), new FollowRefB()]);
"#,
            expected: "56",
        },
    ]);
}
