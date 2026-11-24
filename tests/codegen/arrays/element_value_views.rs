//! Purpose:
//! Regression coverage for boxed values written as container elements: a hash `foreach`
//! binding, a guard-narrowed `mixed` local and a branch merge must keep the container on the
//! element storage the checker typed.
//!
//! Called from:
//! - The `codegen_tests_data` array suite.
//!
//! Key details:
//! - The widened container used to disagree with every consumer compiled against the checked
//!   type: a typed property store was refused at compile time, and a return, parameter or
//!   declared local read boxed cells as raw payloads (`implode` printed pointers).
//! - A branch that leaves a builder empty adopts the other branch's storage instead of boxing
//!   both, which was a second way for the same container to disagree with its checked type.
//! - Heap debugging checks the projected string copies, the extracted rows and the boxes.

use crate::support::*;

/// Runs `source` with and without EIR optimization and requires `expected` and a clean heap.
fn assert_clean_output(source: &str, expected: &str) {
    for optimize in [false, true] {
        let out = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(out.success, "ir-opt={optimize}: {}", out.stderr);
        assert_eq!(out.stdout, expected, "ir-opt={optimize}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir-opt={optimize}: {}", out.stderr);
    }
}

/// Validated JSON preferences fill a declared `array<string, bool>` and reach a typed property:
/// repeated reloads replace the map, invalid keys and values are rejected, and nothing leaks.
#[test]
fn test_element_views_validated_json_map_reaches_typed_property() {
    let source = r#"<?php
final class Settings
{
    private array<string, bool> $values = [];
    private int $loads = 0;

    public function load(string $json): void
    {
        $data = json_decode($json, true, 512, JSON_THROW_ON_ERROR);
        if (!is_array($data)) {
            throw new UnexpectedValueException('Expected an object');
        }
        array<string, bool> $values = [];
        foreach ($data as $name => $value) {
            if (!is_string($name) || !is_bool($value)) {
                throw new UnexpectedValueException('Expected boolean preferences');
            }
            $values[$name] = $value;
        }
        $this->values = $values;
        $this->loads++;
    }

    public function enabled(string $name): bool
    {
        return $this->values[$name] ?? false;
    }

    public function describe(): string
    {
        $parts = [];
        foreach ($this->values as $name => $value) {
            $parts[] = $name . '=' . ($value ? 'on' : 'off');
        }
        return $this->loads . ':' . implode(',', $parts) . ':' . json_encode($this->values)
            . ':' . (in_array(true, $this->values, true) ? 'y' : 'n')
            . ':' . count($this->values);
    }
}

function attempt(Settings $settings, string $json): string
{
    try {
        $settings->load($json);
        return 'ok';
    } catch (UnexpectedValueException $error) {
        return $error->getMessage();
    } catch (JsonException $error) {
        return 'json';
    }
}

$settings = new Settings();
echo attempt($settings, '{"activity":true,"compact":false}'), '|', $settings->describe(), "\n";
echo attempt($settings, '{"activity":1}'), '|', $settings->describe(), "\n";
echo attempt($settings, '[true,false]'), '|', $settings->describe(), "\n";
echo attempt($settings, '{"compact":true,"wide":"yes"}'), '|', $settings->describe(), "\n";
echo attempt($settings, '7'), '|', attempt($settings, '{'), '|', $settings->describe(), "\n";
echo attempt($settings, '{"wide":true}'), '|', $settings->describe(), "\n";
echo $settings->enabled('wide') ? 'on' : 'off', ':', $settings->enabled('activity') ? 'on' : 'off';
unset($settings);
"#;
    let loaded = r#"1:activity=on,compact=off:{"activity":true,"compact":false}:y:2"#;
    let expected = format!(
        "ok|{loaded}\nExpected boolean preferences|{loaded}\nExpected boolean preferences|{loaded}\n\
         Expected boolean preferences|{loaded}\nExpected an object|json|{loaded}\n\
         ok|2:wide=on:{{\"wide\":true}}:y:1\non:off"
    );
    assert_clean_output(source, &expected);
}

/// Declared overrides update an inferred palette that a typed property and a constructor
/// parameter then hold; `with()` copies without touching the source palette or its aliases.
#[test]
fn test_element_views_palette_overrides_keep_typed_map_storage() {
    let source = r#"<?php
final class Palette
{
    private array<string, int> $colors = [];

    public function __construct(array<string, int> $overrides = [])
    {
        $colors = ['base' => 0xFF12_3456, 'accent' => 0xFF70_D7E8];
        foreach ($overrides as $role => $color) {
            if (!isset($colors[$role])) {
                throw new InvalidArgumentException('Unknown color: ' . $role);
            }
            self::validate($color);
            $colors[$role] = $color;
        }
        $this->colors = $colors;
    }

    private static function validate(int $color): void
    {
        if ($color < 0 || $color > 0xFFFF_FFFF) {
            throw new InvalidArgumentException('Expected an ARGB word');
        }
    }

    public function with(array<string, int> $overrides): Palette
    {
        $colors = $this->colors;
        foreach ($overrides as $role => $color) {
            $colors[$role] = $color;
        }
        return new Palette($colors);
    }

    public function color(string $role): int
    {
        return $this->colors[$role];
    }

    public function describe(): string
    {
        return implode(',', $this->colors) . '|' . array_sum($this->colors) . '|'
            . (in_array(0xFF88_8888, $this->colors, true) ? 'y' : 'n') . '|'
            . json_encode(array_values($this->colors));
    }
}

$palette = new Palette(['base' => 0xFF88_8888]);
echo $palette->color('base'), "\n", $palette->describe(), "\n";
$custom = $palette->with(['accent' => 0xFF00_0001]);
echo $custom->describe(), "\n", $palette->describe(), "\n";
try { new Palette(['missing' => 1]); } catch (InvalidArgumentException $error) { echo $error->getMessage(), "\n"; }
try { new Palette(['base' => -1]); } catch (InvalidArgumentException $error) { echo $error->getMessage(), "\n"; }
echo (new Palette())->describe();
unset($palette, $custom, $error);
"#;
    assert_clean_output(source, "4287137928\n\
        4287137928,4285585384|8572723312|y|[4287137928,4285585384]\n\
        4287137928,4278190081|8565328009|y|[4287137928,4278190081]\n\
        4287137928,4285585384|8572723312|y|[4287137928,4285585384]\n\
        Unknown color: missing\nExpected an ARGB word\n\
        4279383126,4285585384|8564968510|n|[4279383126,4285585384]");
}

/// Plain PHP builders filled from a hash `foreach` return the storage their callers read:
/// lists and maps of every scalar, collected keys, copies, branch merges, guards and literals.
#[test]
fn test_element_views_inferred_builders_return_checked_storage() {
    let source = r#"<?php
function listOfInts() { $src = ['a' => 5, 'b' => 6]; $out = []; foreach ($src as $v) { $out[] = $v; } return $out; }
function seededInts() { $src = ['a' => 5, 'b' => 6]; $out = [1, 2]; foreach ($src as $v) { $out[] = $v; } return $out; }
function mapOfInts() { $src = ['a' => 5, 'b' => 6]; $out = ['x' => 1]; foreach ($src as $k => $v) { $out[$k] = $v; } return $out; }
function mapOfStrings() { $src = ['a' => 'p', 'b' => 'q']; $out = ['x' => 'y']; foreach ($src as $k => $v) { $out[$k] = $v; } return $out; }
function listOfFloats() { $src = ['a' => 1.5, 'b' => 2.25]; $out = [0.5]; foreach ($src as $v) { $out[] = $v; } return $out; }
function listOfBools() { $src = ['a' => true, 'b' => false]; $out = [false]; foreach ($src as $v) { $out[] = $v; } return $out; }
function listOfKeys() { $src = ['a' => 1, 'b' => 2]; $out = ['z']; foreach ($src as $k => $v) { $out[] = $k; } return $out; }
function viaCopy() { $src = ['a' => 5, 'b' => 6]; $out = [1]; foreach ($src as $v) { $x = $v; $out[] = $x; } return $out; }
function viaLoopCarriedCopy() { $src = ['a' => 5, 'b' => 6]; $x = 0; $out = []; foreach ($src as $v) { $out[] = $x; $x = $v; } $out[] = $x; return $out; }
function viaTernary(bool $c) { $src = ['a' => 5, 'b' => 6]; $out = [1]; foreach ($src as $v) { $out[] = $c ? $v : 0; } return $out; }
function viaGuard(mixed $x) { $out = [1]; if (is_int($x)) { $out[] = $x; } return $out; }
function viaLiteral() { $src = ['a' => 5, 'b' => 6]; $out = []; foreach ($src as $v) { $out = [$v, $v]; } return $out; }
function viaAssocLiteral() { $src = ['a' => 5, 'b' => 6]; $out = []; foreach ($src as $v) { $out = ['first' => $v, 'second' => $v]; } return $out; }
function show(array $a): string { return implode(',', $a) . '/' . json_encode($a); }
$r = listOfInts(); echo show($r), '|', array_sum($r), '|', in_array(5, $r, true) ? 'y' : 'n', '|', max($r), "\n";
$q = seededInts(); echo show($q), '|', array_sum($q), '|', in_array(5, $q, true) ? 'y' : 'n', '|', array_search(6, $q), "\n";
$m = mapOfInts(); echo show($m), '|', array_sum($m), '|', in_array(5, $m, true) ? 'y' : 'n', '|', json_encode(array_values($m)), '|', array_search(6, $m), '|', $m == mapOfInts() ? 'eq' : 'ne', "\n";
$s = mapOfStrings(); echo show($s), '|', in_array('p', $s, true) ? 'y' : 'n', '|', json_encode(array_values($s)), '|', strlen($s['a']), "\n";
echo show(listOfFloats()), '|', array_sum(listOfFloats()), '|', show(listOfBools()), "\n";
echo show(listOfKeys()), '|', in_array('a', listOfKeys(), true) ? 'y' : 'n', "\n";
echo show(viaCopy()), '|', show(viaLoopCarriedCopy()), '|', show(viaTernary(true)), '|', show(viaTernary(false)), '|', show(viaGuard(7)), '|', show(viaGuard('s')), "\n";
echo show(viaLiteral()), '|', show(viaAssocLiteral()), '|', array_sum(viaAssocLiteral());
unset($r, $q, $m, $s);
"#;
    assert_clean_output(source, "5,6/[5,6]|11|y|6\n\
        1,2,5,6/[1,2,5,6]|14|y|3\n\
        1,5,6/{\"x\":1,\"a\":5,\"b\":6}|12|y|[1,5,6]|b|eq\n\
        y,p,q/{\"x\":\"y\",\"a\":\"p\",\"b\":\"q\"}|y|[\"y\",\"p\",\"q\"]|1\n\
        0.5,1.5,2.25/[0.5,1.5,2.25]|4.25|,1,/[false,true,false]\n\
        z,a,b/[\"z\",\"a\",\"b\"]|y\n\
        1,5,6/[1,5,6]|0,5,6/[0,5,6]|1,5,6/[1,5,6]|1,0,0/[1,0,0]|1,7/[1,7]|1/[1]\n\
        6,6/[6,6]|6,6/{\"first\":6,\"second\":6}|12");
}

/// Rows read from a `foreach` binding are appended as the arrays they are, from an indexed
/// and from a string-keyed source, into a seeded and into an empty builder.
#[test]
fn test_element_views_nested_rows_return_checked_storage() {
    let source = r#"<?php
function rows() { $src = [[1, 2], [3]]; $out = [[9]]; foreach ($src as $r) { $out[] = $r; } return $out; }
function hashRows() { $src = ['a' => [1, 2], 'b' => [3]]; $out = []; foreach ($src as $r) { $out[] = $r; } return $out; }
function keyedRows() { $src = ['a' => ['x' => 1], 'b' => ['y' => 2]]; $out = ['seed' => ['z' => 0]]; foreach ($src as $k => $r) { $out[$k] = $r; } return $out; }
foreach (rows() as $row) { echo implode(',', $row), ';'; }
echo '|';
foreach (hashRows() as $row) { echo implode(',', $row), ';'; }
echo '|', json_encode(keyedRows()), '|', count(rows()), ':', array_sum(hashRows()[0]);
unset($row);
"#;
    assert_clean_output(source,
        "9;1,2;3;|1,2;3;|{\"seed\":{\"z\":0},\"a\":{\"x\":1},\"b\":{\"y\":2}}|3:3");
}

/// A declared return, parameter, local and property all read the scalar storage they declare.
#[test]
fn test_element_views_declared_boundaries_read_typed_storage() {
    let source = r#"<?php
function build(array<string, int> $o): array<string, int> {
    $colors = ['base' => 1, 'accent' => 2];
    foreach ($o as $role => $color) { $colors[$role] = $color; }
    return $colors;
}
function buildList(array<string, int> $o): array<int> {
    $out = [1, 2];
    foreach ($o as $v) { $out[] = $v; }
    return $out;
}
function take(array<string, int> $m): string { return implode(',', $m) . '/' . array_sum($m) . '/' . ($m['base'] + 1); }
function takeList(array<int> $a): string { return implode(',', $a) . '/' . array_sum($a) . '/' . ($a[2] + 1); }
function viaParam(array<string, int> $o): string {
    $colors = ['base' => 1, 'accent' => 2];
    foreach ($o as $role => $color) { $colors[$role] = $color; }
    $out = [1, 2];
    foreach ($o as $v) { $out[] = $v; }
    return take($colors) . '|' . takeList($out);
}
function viaLocal(array<string, int> $o): string {
    $colors = ['base' => 1, 'accent' => 2];
    foreach ($o as $role => $color) { $colors[$role] = $color; }
    array<string, int> $typed = $colors;
    $out = [1, 2];
    foreach ($o as $v) { $out[] = $v; }
    array<int> $list = $out;
    return implode(',', $typed) . '/' . (in_array(5, $typed, true) ? 'y' : 'n') . '|'
        . implode(',', $list) . '/' . (in_array(5, $list, true) ? 'y' : 'n');
}
final class Box {
    public array<int> $items = [];
    public array<string, int> $map = [];
    public function fill(array<string, int> $o): void {
        $out = [1, 2];
        $map = ['base' => 1];
        foreach ($o as $k => $v) { $out[] = $v; $map[$k] = $v; }
        $this->items = $out;
        $this->map = $map;
    }
}
$r = build(['base' => 5]);
echo implode(',', $r), '|', array_sum($r), '|', in_array(5, $r, true) ? 'y' : 'n', '|', json_encode(array_values($r)), '|', array_search(2, $r), '|', $r == build(['base' => 5]) ? 'eq' : 'ne', "\n";
$l = buildList(['a' => 5]);
echo implode(',', $l), '|', array_sum($l), '|', in_array(5, $l, true) ? 'y' : 'n', "\n";
echo viaParam(['base' => 5]), "\n", viaLocal(['base' => 5]), "\n";
$b = new Box(); $b->fill(['base' => 5, 'extra' => 7]);
echo implode(',', $b->items), '|', implode(',', $b->map), '|', array_sum($b->items) + array_sum($b->map);
unset($r, $l, $b);
"#;
    assert_clean_output(source, "5,2|7|y|[5,2]|accent|eq\n\
        1,2,5|8|y\n\
        5,2/7/6|1,2,5/8/6\n\
        5,2/y|1,2,5/y\n\
        1,2,5,7|5,7|27");
}

/// A global and a by-reference captured local are boxed cells other code can rewrite: the
/// container widens in the checker too, so callers read the runtime value, clobbered or not.
#[test]
fn test_element_views_volatile_cells_widen_the_checked_container() {
    let source = r#"<?php
$counter = 5;
$label = 'top';
function fromGlobal() { global $counter; $out = [1]; $out[] = $counter; return $out; }
function fromGlobalMap() { global $label; $out = ['seed' => 'x']; $out['label'] = $label; return $out; }
function fromGlobalLiteral() { global $counter; return [$counter, $counter + 1]; }
function fromCapture() { $x = 5; $inc = function() use (&$x) { $x++; }; $inc(); $out = [1]; $out[] = $x; return $out; }
function clobber(): void { global $shared; $shared = 'abc'; }
function fromClobbered() { global $shared; $shared = 7; $out = [1]; clobber(); $out[] = $shared; return $out; }
function inLoop() { global $counter; $out = [1]; for ($i = 0; $i < 2; $i++) { $out[] = $counter; } return $out; }
function show(array $a): string { return implode(',', $a) . '/' . json_encode($a); }
echo show(fromGlobal()), '|', in_array(5, fromGlobal(), true) ? 'y' : 'n', "\n";
echo show(fromGlobalMap()), '|', in_array('top', fromGlobalMap(), true) ? 'y' : 'n', "\n";
echo show(fromGlobalLiteral()), '|', array_sum(fromGlobalLiteral()), "\n";
echo show(fromCapture()), '|', in_array(6, fromCapture(), true) ? 'y' : 'n', "\n";
echo show(fromClobbered()), '|', gettype(fromClobbered()[1]), "\n";
echo show(inLoop()), '|', array_sum(inLoop()), "\n";
$top = [1]; $top[] = $counter; echo show($top), '|', in_array(5, $top, true) ? 'y' : 'n';
unset($top, $counter, $label);
"#;
    assert_clean_output(source, "1,5/[1,5]|y\n\
        x,top/{\"seed\":\"x\",\"label\":\"top\"}|y\n\
        5,6/[5,6]|11\n\
        1,6/[1,6]|y\n\
        1,abc/[1,\"abc\"]|string\n\
        1,5,5/[1,5,5]|11\n\
        1,5/[1,5]|y");
}

/// A builder that is still empty on one branch adopts the storage the other branch gave it:
/// filters over lists and hashes, keyed builders, single branches, both branches and ternaries.
#[test]
fn test_element_views_conditional_builders_keep_checked_storage() {
    let source = r#"<?php
function evens() { $out = []; foreach ([1, 2, 3, 4] as $v) { if ($v % 2 === 0) { $out[] = $v; } } return $out; }
function labels() { $out = []; foreach ([1, 2, 3] as $v) { if ($v > 1) { $out[] = str_repeat('x', $v); } } return $out; }
function keyed() { $out = []; foreach ([1, 2, 3] as $v) { if ($v > 1) { $out['k' . $v] = $v; } } return $out; }
function fromHash() { $src = ['a' => 1, 'b' => 2, 'c' => 3]; $out = []; foreach ($src as $k => $v) { if ($v > 1) { $out[] = $v; } } return $out; }
function names() { $src = ['a' => true, 'b' => false, 'c' => true]; $on = []; foreach ($src as $name => $enabled) { if ($enabled) { $on[] = $name; } } return $on; }
function once(bool $c) { $out = []; if ($c) { $out[] = 7; } return $out; }
function onceKeyed(bool $c) { $out = []; if ($c) { $out['k'] = 'v'; } return $out; }
function either(bool $c) { $out = []; if ($c) { $out[] = 1.5; } else { $out[] = 2.5; } return $out; }
function ternary(bool $c) { $out = []; $c ? ($out[] = 3) : 0; return $out; }
function mixedArms(bool $c) { $out = []; if ($c) { $out[] = 1; } else { $out[] = 's'; } return $out; }
function show(array $a): string { return implode(',', $a) . '/' . json_encode($a); }
echo show(evens()), '|', in_array(2, evens(), true) ? 'y' : 'n', '|', array_sum(evens()), "\n";
echo show(labels()), '|', in_array('xx', labels(), true) ? 'y' : 'n', "\n";
echo show(keyed()), '|', in_array(3, keyed(), true) ? 'y' : 'n', '|', json_encode(array_values(keyed())), "\n";
echo show(fromHash()), '|', in_array(3, fromHash(), true) ? 'y' : 'n', '|', show(names()), '|', in_array('c', names(), true) ? 'y' : 'n', "\n";
echo show(once(true)), '|', show(once(false)), '|', show(onceKeyed(true)), '|', show(onceKeyed(false)), '|', array_sum(once(true)), "\n";
echo show(either(true)), '|', show(either(false)), '|', show(ternary(true)), '|', show(ternary(false)), '|', show(mixedArms(true)), '|', show(mixedArms(false));
"#;
    assert_clean_output(source, "2,4/[2,4]|y|6\n\
        xx,xxx/[\"xx\",\"xxx\"]|y\n\
        2,3/{\"k2\":2,\"k3\":3}|y|[2,3]\n\
        2,3/[2,3]|y|a,c/[\"a\",\"c\"]|y\n\
        7/[7]|/[]|v/{\"k\":\"v\"}|/[]|7\n\
        1.5/[1.5]|2.5/[2.5]|3/[3]|/[]|1/[1]|s/[\"s\"]");
}

/// A detached string copy that ends up in a boxed container is still released after the write.
#[test]
fn test_element_views_projected_strings_release_after_boxed_writes() {
    let source = r#"<?php
function cast(mixed $m): int { $out = [1]; $out[] = (string) $m; return count($out); }
function keys(array<string, bool> $flags): array { $out = [1]; foreach ($flags as $name => $on) { $out[] = $name; } return $out; }
function build(): array<string, bool> { array<string, bool> $flags = []; $flags[str_repeat('a', 5)] = true; $flags[str_repeat('b', 6)] = true; return $flags; }
echo cast(str_repeat('a', 5)), '|', json_encode(keys(build()));
"#;
    assert_clean_output(source, "2|[1,\"aaaaa\",\"bbbbbb\"]");
}

/// A local assigned `null` and then a string in one branch may still hold the `null`: its
/// checked type is not a proof, so a container written from it keeps the runtime value.
#[test]
fn test_element_views_unproven_locals_keep_runtime_values() {
    let source = r#"<?php
$outer = function (int $n) {
    $inner = function (int $k) { $s = null; if ($k > 1) { $s = "in" . $k; } return $s; };
    $t = null;
    if ($n > 0) { $t = $inner($n + 1); }
    $list = [1];
    $list[] = $t;
    $list[] = $inner(0);
    $map = ['seed' => 'x'];
    $map['t'] = $t;
    return [[$t, $inner(0)], $list, $map];
};
echo json_encode($outer(1)), '|', json_encode($outer(0));
unset($outer);
"#;
    assert_clean_output(source,
        "[[\"in2\",null],[1,\"in2\",null],{\"seed\":\"x\",\"t\":\"in2\"}]|\
         [[null,null],[1,null,null],{\"seed\":\"x\",\"t\":null}]");
}

/// A branch result used as a literal item or as an appended value is the proven scalar on
/// every arm, in indexed and keyed literals, nested merges and spread literals alike.
#[test]
fn test_element_views_branch_results_keep_literal_storage() {
    let source = r#"<?php
function single(bool $c) { foreach (['a' => 5] as $v) { return [$c ? $v : 0]; } return [0]; }
function keyed(bool $c) { $src = ['a' => 5, 'b' => 6]; $out = []; foreach ($src as $k => $v) { $out = ['v' => $c ? $v : 0, 'w' => $v ?: 1]; } return $out; }
function nested(bool $c, bool $d) { $src = ['a' => 5]; $out = [1]; foreach ($src as $v) { $out[] = $c ? ($d ? $v : 2) : 3; $out = [...$out, $c ? $v : 0]; } return $out; }
function strings(bool $c) { $src = ['a' => str_repeat('p', 3)]; foreach ($src as $k => $v) { return ['value' => $c ? $v : 'none', 'key' => $c ? $k : 'nokey']; } return ['value' => 'x', 'key' => 'y']; }
$single = single(true); $keyed = keyed(true); $nested = nested(true, true); $strings = strings(true);
echo implode(',', $single), '|', in_array(5, $single, true) ? 'y' : 'n', '|', array_sum($single), '|', implode(',', single(false)), "\n";
echo implode(',', $keyed), '|', in_array(6, $keyed, true) ? 'y' : 'n', '|', array_sum($keyed), '|', json_encode(keyed(false)), "\n";
echo implode(',', $nested), '|', in_array(5, $nested, true) ? 'y' : 'n', '|', implode(',', nested(true, false)), '|', implode(',', nested(false, true)), "\n";
echo implode(',', $strings), '|', in_array('ppp', $strings, true) ? 'y' : 'n', '|', json_encode(strings(false));
unset($single, $keyed, $nested, $strings);
"#;
    assert_clean_output(source, "5|y|5|0\n\
        6,6|y|12|{\"v\":0,\"w\":6}\n\
        1,5,5|y|1,2,5|1,3,0\n\
        ppp,a|y|{\"value\":\"none\",\"key\":\"nokey\"}");
}

/// A literal assigned to a fresh local, with no seed and no declared return to rebuild it, is
/// read by its callers as the type the checker gave it: `??`, `?:` and `match` arms that read
/// a loop binding, and a ternary whose other arm is a call. The bool source holds a `bool`
/// value, not the literal `false`, whose own marker type is a separate checker gap.
#[test]
fn test_element_views_merge_arms_keep_unseeded_literal_storage() {
    let source = r#"<?php
function intCoalesce() { foreach (['a' => 5] as $v) { $out = [$v ?? 0]; } return $out; }
function floatCoalesce() { foreach (['a' => 1.5] as $v) { $out = [$v ?? 2.0]; } return $out; }
function stringCoalesce() { foreach (['a' => str_repeat('p', 3)] as $v) { $out = [$v ?? 'x']; } return $out; }
function boolPair(bool $t) { $src = ['a' => $t]; foreach ($src as $v) { $out = [$v ?? true, $v ?: true]; } return $out; }
function mixedOrder() { foreach (['a' => 0] as $v) { $out = [$v ?? 7, $v ?: 1]; } return $out; }
function nestedCoalesce() { foreach (['a' => 5] as $v) { $out = [true ? ($v ?? 0) : 1]; } return $out; }
function matchArm(bool $c) { foreach (['a' => 5] as $v) { $out = [match ($c) { true => $v, default => 0 }]; } return $out; }
function callSibling(bool $c) { foreach (['a' => 5] as $v) { $out = [$c ? $v : strlen('ab')]; } return $out; }
function callBeside(bool $c) { foreach (['a' => 5] as $v) { $out = [$v, $c ? $v : strlen('ab')]; } return $out; }
function keyedCoalesce() { foreach (['a' => 5] as $k => $v) { $out = ['v' => $v ?? 0, 'k' => $k ?? 'none']; } return $out; }
$i = intCoalesce(); $f = floatCoalesce(); $s = stringCoalesce(); $b = boolPair(false); $m = mixedOrder();
echo implode(',', $i), '|';
foreach ($f as $x) { echo $x + 0.25, ';'; }
echo '|';
foreach ($s as $x) { echo $x, ':', strlen($x), ';'; }
echo '|', implode(',', $b), '|', in_array(true, $b, true) ? 'y' : 'n', in_array(false, $b, true) ? 'y' : 'n', '|', implode(',', $m), '|', implode(',', nestedCoalesce()), "\n";
echo implode(',', matchArm(true)), '|', implode(',', matchArm(false)), '|', implode(',', callSibling(true)), '|', implode(',', callSibling(false)), '|', implode(',', callBeside(true)), '|', implode(',', callBeside(false)), '|', array_sum(callBeside(true)), "\n";
$k = keyedCoalesce(); echo $k['v'] + 1, ':', strlen($k['k']), ':', $k['k'];
unset($i, $f, $s, $b, $m, $k, $x);
"#;
    assert_clean_output(source, "5|1.75;|ppp:3;|,1|yy|0,1|5\n\
        5|0|5|2|5,5|5,2|10\n\
        6:1:a");
}

/// A proof recorded in one branch does not reach its sibling: a copy of a loop binding in the
/// `if` arm must not let the `else` arm cast the `null` the local still holds there, nor the
/// code after the branch, a later `switch` case, a `catch` block or the code after a `for`
/// whose update made the copy.
#[test]
fn test_element_views_branch_proofs_stay_in_their_branch() {
    let source = r#"<?php
function sibling(bool $c, int $n) {
    $text = null;
    if ($n > 0) { $text = 's'; }
    $out = [1];
    foreach (['a' => 'value'] as $value) {
        if ($c) { $text = $value; }
        else { $out[] = $text; }
    }
    return $out;
}
function afterBranch(bool $c, int $n) {
    $text = null;
    if ($n > 0) { $text = 's'; }
    $out = [1];
    foreach (['a' => 'value'] as $value) {
        if ($c) { $text = $value; }
        $out[] = $text;
    }
    return $out;
}
function laterCase(int $k, int $n) {
    $text = null;
    if ($n > 0) { $text = 's'; }
    $out = [1];
    foreach (['a' => 'value'] as $value) {
        switch ($k) {
            case 1: $text = $value; break;
            default: $out[] = $text;
        }
    }
    return $out;
}
function inCatch(bool $c, int $n) {
    $text = null;
    if ($n > 0) { $text = 's'; }
    $out = [1];
    foreach (['a' => 'value'] as $value) {
        try {
            if ($c) { throw new RuntimeException('skip'); }
            $text = $value;
        } catch (RuntimeException $e) {
            $out[] = $text;
        }
    }
    return $out;
}
function forUpdate(int $n) {
    $text = null;
    if ($n > 5) { $text = 's'; }
    $out = [1];
    foreach (['a' => 'value'] as $value) {
        for ($i = 0; $i < $n; $text = $value) { $i++; }
        $out[] = $text;
    }
    return $out;
}
function sameBranch(bool $c) {
    $out = ['seed'];
    foreach (['a' => 'value'] as $value) {
        if ($c) { $text = $value; $out[] = $text; }
    }
    return $out;
}
echo json_encode(sibling(false, 0)), json_encode(sibling(false, 1)), json_encode(sibling(true, 0)), '|';
echo json_encode(afterBranch(false, 0)), json_encode(afterBranch(true, 0)), '|';
echo json_encode(laterCase(2, 0)), json_encode(laterCase(2, 1)), '|';
echo json_encode(inCatch(true, 0)), json_encode(inCatch(false, 0)), '|';
echo json_encode(forUpdate(0)), json_encode(forUpdate(2)), '|';
$same = sameBranch(true); echo implode(',', $same), ':', strlen($same[1]), in_array('value', $same, true) ? 'y' : 'n';
unset($same);
"#;
    assert_clean_output(source, "[1,null][1,\"s\"][1]|[1,null][1,\"value\"]|\
        [1,null][1,\"s\"]|[1,null][1]|[1,null][1,\"value\"]|seed,value:5y");
}

/// A proof every path to the next statement holds survives the statement: both arms of an
/// `if`/`else`, the one arm that falls through, a `try` body that completed (with or without
/// a `finally`), and a `try` with a `catch` that records the same proof. A `finally` that
/// stores the local ends it, and an arm that records nothing leaves the local unproven.
#[test]
fn test_element_views_joined_branch_proofs() {
    let source = r#"<?php
function tryFinally() {
    $out = [1];
    foreach (['a' => 5] as $v) {
        try { $copy = $v; }
        finally { echo '.'; }
        $out[] = $copy;
    }
    return $out;
}
function bothArms(bool $c) {
    $out = [1];
    foreach (['a' => 5, 'b' => 7] as $v) {
        if ($c) { $copy = $v; } else { $copy = $v; }
        $out[] = $copy;
    }
    return $out;
}
function exitingArm(bool $c) {
    $out = ['x'];
    foreach (['a' => 'p', 'b' => 'q'] as $k => $v) {
        if ($c && $k === 'a') { continue; } else { $t = $v; }
        $out[] = $t;
    }
    return $out;
}
function tryCatchBoth(bool $c) {
    $out = ['x'];
    foreach (['a' => 'p', 'b' => 'q'] as $v) {
        try {
            if ($c) { throw new RuntimeException('skip'); }
            $t = $v;
        } catch (RuntimeException $e) {
            $t = $v;
        }
        $out[] = $t;
    }
    return $out;
}
function finallyStores() {
    $out = ['x'];
    foreach (['a' => 'p'] as $v) {
        try { $t = $v; }
        finally { $t = 'f'; }
        $out[] = $t;
    }
    return $out;
}
function oneArmOnly(bool $c, int $n) {
    $text = null;
    if ($n > 0) { $text = 's'; }
    $out = [1];
    foreach (['a' => 'value'] as $value) {
        if ($c) { $text = $value; } else { echo ''; }
        $out[] = $text;
    }
    return $out;
}
$a = tryFinally(); echo implode(',', $a), ' ', $a[1] + 1, in_array(5, $a, true) ? 'y' : 'n', '|';
$b = bothArms(true); echo implode(',', $b), ' ', $b[2] + 1, in_array(7, $b, true) ? 'y' : 'n', '|';
$b = bothArms(false); echo implode(',', $b), ' ', $b[1] + 1, in_array(5, $b, true) ? 'y' : 'n', '|';
$e = exitingArm(true); echo implode(',', $e), ' ', strlen($e[1]), in_array('q', $e, true) ? 'y' : 'n', '|';
$t = tryCatchBoth(true); echo implode(',', $t), ' ', strlen($t[2]), in_array('q', $t, true) ? 'y' : 'n', '|';
$t = tryCatchBoth(false); echo implode(',', $t), ' ', strlen($t[1]), in_array('p', $t, true) ? 'y' : 'n', '|';
$f = finallyStores(); echo implode(',', $f), ' ', strlen($f[1]), in_array('f', $f, true) ? 'y' : 'n', '|';
echo json_encode(oneArmOnly(false, 0)), json_encode(oneArmOnly(true, 0));
unset($a, $b, $e, $t, $f);
"#;
    assert_clean_output(
        source,
        ".1,5 6y|1,5,7 8y|1,5,7 6y|x,q 1y|x,p,q 1y|x,p,q 1y|x,f 1y|[1,null][1,\"value\"]",
    );
}
