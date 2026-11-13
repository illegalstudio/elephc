//! Purpose:
//! Regression coverage for the integer key of a by-value `foreach`, which now rewrites the
//! Mixed cell its key variable already owns instead of allocating one cell per entry
//! (task b58e5006).
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Reuse is decided at run time from the cell's reference count, so every fixture that
//!   keeps a key alive (an array element, another variable, a closure, a static property)
//!   must still see its own value afterwards.
//! - Expected output comes from PHP 8.5; `ELEPHC_PHP_CHECK=1` re-runs the oracle.
//! - Every fixture runs in both EIR modes with heap checks.

use super::*;

/// Cross-checks PHP-compatible fixtures when the focused run opts into the PHP oracle.
fn php_oracle(source: &str, expected: &str) {
    if std::env::var_os("ELEPHC_PHP_CHECK").is_none() { return; }
    let output = Command::new("php").args(["-n", "-r", source.trim_start_matches("<?php")])
        .output().expect("run PHP foreach key oracle");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

/// Compiles one variant, validates its heap and returns stdout.
fn run(source: &str, optimized: bool) -> String {
    let dir = make_cli_test_dir("elephc_foreach_key_cells");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.arg("--heap-debug");
    if !optimized { command.arg("--no-ir-opt"); }
    let compile = command.arg(&path).output().unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    fs::remove_dir_all(&dir).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    stdout
}

/// Keys that outlive their iteration, displaced bindings, nested and repeated key variables,
/// captures, generators, exceptions and string keys all behave as in PHP.
#[test]
fn test_foreach_key_cells_preserve_php_key_semantics() {
    let source = r#"<?php
class Noisy { public function __construct(public string $n) {} public function __destruct() { echo "drop ", $this->n, "\n"; } }
class Sink { public static array $kept = []; public static function keep(mixed $v): void { self::$kept[] = $v; } }
function show(string $label, array $values): void { echo $label, "=", implode(",", $values), "\n"; }
function collect(array $src): array { $keys = []; foreach ($src as $k => $v) { $keys[] = $k; } return $keys; }
function last(array $src): mixed { $last = null; foreach ($src as $k => $v) { $last = $k; } return $last; }
function afterLoop(array $src): mixed { foreach ($src as $k => $v) { if ($v === 'stop') { break; } } return $k; }
function emptyLoop(array $src): void { foreach ($src as $k => $v) { echo "never\n"; } var_dump($k ?? null); }
function displaced(array $src): void { $k = new Noisy('before'); foreach ($src as $k => $v) { echo "k=", $k, "\n"; } echo "after loop\n"; }
function strBefore(array $src): void { $k = str_repeat('x', 3); foreach ($src as $k => $v) { echo $k, ","; } echo "\n"; var_dump($k); }
function nested(array $a, array $b): void { foreach ($a as $k => $v) { foreach ($b as $k => $w) { echo $k, $w, " "; } echo "[", $k, "]"; } echo "\n"; }
function byRefCapture(array $src): array { $fns = []; foreach ($src as $k => $v) { $fns[] = function () use (&$k) { return $k; }; } $out = []; foreach ($fns as $f) { $out[] = $f(); } return $out; }
function byValCapture(array $src): array { $fns = []; foreach ($src as $k => $v) { $fns[] = fn() => $k; } $out = []; foreach ($fns as $f) { $out[] = $f(); } return $out; }
function gen(array $src) { foreach ($src as $k => $v) { yield $k; } }
function thrower(array $src): mixed { try { foreach ($src as $k => $v) { if ($k === 7) { throw new Exception('at ' . $k); } } } catch (Exception $e) { echo $e->getMessage(), "\n"; } return $k; }
function staticKeep(array $src): void { foreach ($src as $k => $v) { Sink::keep($k); } }
function mutate(array $src): array { $out = []; foreach ($src as $k => $v) { $out[] = $k; $k = $k * 10; $out[] = $k; } return $out; }
function rebuild(array $src): array { $dst = []; foreach ($src as $k => $v) { $dst[$k] = $v; } return $dst; }
function sumKeys(array $src): int { $sum = 0; foreach ($src as $k => $v) { $sum += $k; } return $sum; }
function mixedKeys(): void { $m = [1 => 'a', 'x' => 'b', 2 => 'c', 'y' => 'd', 5 => 'e']; $seen = []; foreach ($m as $k => $v) { $seen[] = $k; echo gettype($k), ":", $k, " "; } echo "\n"; show('seen', $seen); }
function continueBreak(array $src): array { $out = []; foreach ($src as $k => $v) { if ($k % 2 === 0) { continue; } if ($k > 6) { break; } $out[] = $k; } $out[] = $k; return $out; }
function refAlias(array $src): mixed { $alias = null; foreach ($src as $k => $v) { $alias = &$k; } $alias = 'changed'; return $k; }
function twoLoops(array $a, array $b): array { $out = []; foreach ($a as $i => $x) { $out[] = $i; } foreach ($b as $i => $y) { $out[] = $i; } $out[] = $i; return $out; }
function holdOld(array $src): array { $held = []; $prev = null; foreach ($src as $k => $v) { if ($prev !== null) { $held[] = $prev . ">" . $k; } $prev = $k; } return $held; }
function sameName(array $src): mixed { foreach ($src as $k => $k) { echo $k, ";"; } echo "\n"; return $k; }
function floats(): array { $out = []; $k = 1.5; foreach ([4 => 'a', 6 => 'b'] as $k => $v) { $out[] = $k; } $out[] = $k; return $out; }
$list = ['a', 'b', 'stop', 'd'];
$map = [3 => 'a', 7 => 'b', 9 => 'stop', 12 => 'd'];
show('list', collect($list)); show('map', collect($map));
var_dump(last($list), last($map), afterLoop($list), afterLoop($map));
emptyLoop(array_slice($list, 0, $argc - 1));
displaced($map); strBefore($list); nested($list, [5 => 'p', 6 => 'q']);
show('byref', byRefCapture($map)); show('byval', byValCapture($map));
foreach (gen($map) as $g) { echo $g, " "; } echo "\n";
var_dump(thrower($map)); staticKeep($map); staticKeep($list); show('kept', Sink::$kept);
show('mutate', mutate($map)); show('rebuild', rebuild($map)); echo sumKeys($map), " ", sumKeys($list), "\n"; mixedKeys();
show('flow', continueBreak([1 => 1, 2 => 2, 3 => 3, 4 => 4, 5 => 5, 6 => 6, 7 => 7, 8 => 8])); var_dump(refAlias($map));
var_dump(sameName($map), sameName($list)); show('two', twoLoops($list, $map)); show('held', holdOld($map)); show('floats', floats());
$top = []; foreach ($map as $tk => $tv) { $top[] = $tk; } show('top', $top); var_dump($tk);
"#;
    let expected = r#"list=0,1,2,3
map=3,7,9,12
int(3)
int(12)
int(2)
int(9)
NULL
drop before
k=3
k=7
k=9
k=12
after loop
0,1,2,3,
int(3)
5p 6q [6]5p 6q [6]5p 6q [6]5p 6q [6]
byref=12,12,12,12
byval=3,7,9,12
3 7 9 12 
at 7
int(7)
kept=3,7,9,12,0,1,2,3
mutate=3,30,7,70,9,90,12,120
rebuild=a,b,stop,d
31 6
integer:1 string:x integer:2 string:y integer:5 
seen=1,x,2,y,5
flow=1,3,5,7
string(7) "changed"
3;7;9;12;
0;1;2;3;
int(12)
int(3)
two=0,1,2,3,3,7,9,12,12
held=3>7,7>9,9>12
floats=4,6,6
top=3,7,9,12
int(12)
"#;
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// Keyed scans of a typed object hash and vector allocate a fixed number of cells, not one per
/// entry, whether the key is copied to a buffer, unused, or only read on a cold branch.
#[test]
fn test_foreach_key_cells_do_not_allocate_per_entry() {
    let source = r#"<?php
abstract class Item { abstract public function tick(): void; abstract public function removing(): bool; }
final class Idle extends Item {
    private bool $gone = false;
    public function tick(): void {}
    public function removing(): bool { return $this->gone; }
}
final class Items {
    private array<int, Item> $map = [];
    private array<Item> $sequence = [];
    public function add(int $id, Item $item): void { $this->map[$id] = $item; $this->sequence[] = $item; }
    public function values(): void { $items = $this->map; foreach ($items as $item) { $item->tick(); } }
    public function keys(buffer<int> $out): void {
        $items = $this->map; $index = 0;
        foreach ($items as $id => $item) { $out[$index] = $id; $index++; $item->tick(); }
    }
    public function unused(): void { $items = $this->map; foreach ($items as $id => $item) { $item->tick(); } }
    public function constant(buffer<int> $out): void { $items = $this->map; foreach ($items as $id => $item) { $out[0] = $id; $item->tick(); } }
    public function cold(): void { $items = $this->map; foreach ($items as $id => $item) { if ($item->removing()) { unset($this->map[$id]); } } }
    public function vector(buffer<int> $out): void {
        $items = $this->sequence; $index = 0;
        foreach ($items as $id => $item) { $out[$index] = $id; $index++; $item->tick(); }
    }
}
function run(int $count): void {
    $items = new Items(); $keys = buffer_new<int>($count);
    for ($i = 0; $i < $count; $i++) { $items->add($i + 1, new Idle()); }
    $before = memory_get_allocations(); $items->values(); $after = memory_get_allocations();
    echo "values allocations=", $after - $before, "\n";
    $before = memory_get_allocations(); $items->keys($keys); $after = memory_get_allocations();
    echo "hash-keys allocations=", $after - $before, " checksum=", $keys[0] + $keys[$count - 1], "\n";
    $before = memory_get_allocations(); $items->unused(); $after = memory_get_allocations();
    echo "unused-key allocations=", $after - $before, "\n";
    $before = memory_get_allocations(); $items->constant($keys); $after = memory_get_allocations();
    echo "constant-out allocations=", $after - $before, " checksum=", $keys[0], "\n";
    $before = memory_get_allocations(); $items->cold(); $after = memory_get_allocations();
    echo "cold-erase allocations=", $after - $before, "\n";
    $before = memory_get_allocations(); $items->vector($keys); $after = memory_get_allocations();
    echo "vector-keys allocations=", $after - $before, " checksum=", $keys[0] + $keys[$count - 1], "\n";
    buffer_free($keys);
}
run(999 + $argc);
"#;
    let expected = "values allocations=0\n\
hash-keys allocations=1 checksum=1001\n\
unused-key allocations=1\n\
constant-out allocations=1 checksum=1000\n\
cold-erase allocations=1\n\
vector-keys allocations=1 checksum=999\n";
    assert_eq!(run(source, true), expected);
    let unoptimized = run(source, false);
    for line in ["unused-key allocations=1\n", "constant-out allocations=1 checksum=1000\n", "cold-erase allocations=1\n"] {
        assert!(unoptimized.contains(line), "{unoptimized}");
    }
    assert!(unoptimized.contains(" checksum=1001\n") && unoptimized.ends_with(" checksum=999\n"), "{unoptimized}");
}

/// A key that another owner still holds is never rewritten: each stored key keeps its value,
/// while a loop that only reads its key allocates a fixed number of cells.
#[test]
fn test_foreach_key_cells_leave_shared_keys_untouched() {
    let source = r#"<?php
function retained(array $src): void {
    $held = [];
    $before = memory_get_allocations();
    foreach ($src as $k => $v) { $held[] = $k; }
    $after = memory_get_allocations();
    echo implode(",", $held), " retained=", $after - $before >= count($src) ? "per-entry" : "shared", "\n";
}
function observed(array $src): void {
    $sum = 0;
    $before = memory_get_allocations();
    foreach ($src as $k => $v) { $sum = $sum + $k; }
    $after = memory_get_allocations();
    echo $sum, " observed=", $after - $before < count($src) ? "fixed" : "per-entry", "\n";
}
$map = [];
for ($i = 0; $i < 6; $i++) { $map[$i * 3 + $argc] = "v" . $i; }
retained($map);
observed($map);
"#;
    for optimized in [false, true] {
        let output = run(source, optimized);
        assert!(output.starts_with("1,4,7,10,13,16 retained=per-entry\n51 observed="), "optimized={optimized}: {output}");
        if optimized {
            assert!(output.ends_with("51 observed=fixed\n"), "{output}");
        }
    }
}

/// Every supported target boxes the keys of a typed and of an untyped array source through
/// the reuse helper, and carries that helper in its runtime.
#[test]
fn test_foreach_key_cells_emit_on_every_target() {
    let source = r#"<?php
function typed(array<int, string> $src): int { $sum = 0; foreach ($src as $k => $v) { $sum = $sum + $k; } return $sum; }
function untyped(array $src): int { $sum = 0; foreach ($src as $k => $v) { $sum = $sum + $k; } return $sum; }
function dynamic(iterable $src): int { $sum = 0; foreach ($src as $k => $v) { $sum = $sum + $k; } return $sum; }
echo typed([$argc => "a"]), untyped([$argc => "a"]), dynamic([$argc => "a"]);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let dir = make_cli_test_dir("elephc_foreach_key_cells_asm");
        let path = dir.join("main.php");
        fs::write(&path, source).unwrap();
        let mut command = elephc_cli_command(&dir);
        command.args(["--emit-asm", "--target", name]);
        if name.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
        let output = command.arg(&path).output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        let assembly = fs::read_to_string(path.with_extension("s")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        for (function, reused) in [("typed", true), ("untyped", true), ("dynamic", false)] {
            let start = assembly.find(&format!("@fn name={function} ")).expect("function start marker");
            let end = start + assembly[start..].find(&format!("@endfn name={function}")).expect("function end marker");
            assert_eq!(assembly[start..end].contains("__rt_mixed_reuse_int_cell"), reused, "{name} {function}");
        }
        let target = Target::parse(name).expect("supported target");
        let runtime = elephc::codegen::generate_runtime(8_388_608, target);
        assert!(runtime.contains("__rt_mixed_reuse_int_cell:"), "{name} runtime lacks the reuse helper");
    }
}
