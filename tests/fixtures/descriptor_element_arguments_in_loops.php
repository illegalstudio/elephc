<?php
// The passing-mode query path: unknown callables called with erased element/property receivers
// while SimpleXMLElement exists in the program, inside loops, with every callee kind.
final class Holder { public $tag = 'h'; }
final class Bag implements ArrayAccess {
    public array $d = ['k' => 'bag-k', 'o' => null];
    public function offsetExists(mixed $o): bool { return isset($this->d[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->d[$o] ?? null; }
    public function offsetSet(mixed $o, mixed $v): void { $this->d[$o] = $v; }
    public function offsetUnset(mixed $o): void { unset($this->d[$o]); }
}
final class Box { public $items = []; public $cell = null; public function __construct() { $this->items = [[1, 'two', [3], new Holder, null]]; $this->cell = [[10, 'x']]; } }
$xml = new SimpleXMLElement('<r><c>old</c><n>5</n></r>');

function show(mixed $v): string {
    if (is_object($v)) { return 'obj:' . get_class($v); }
    if (is_array($v)) { return 'arr:' . json_encode($v); }
    return var_export($v, true);
}
$byVal = static fn ($v) => 'val(' . show($v) . ')';
$byRef = static function (&$v) { $r = 'ref(' . show($v) . ')'; return $r; };
$bump = static function (&$v) { if (is_int($v)) { $v++; } elseif (is_string($v)) { $v .= '!'; } return 'bump'; };
$thrower = static function ($v) { throw new RuntimeException('boom:' . show($v)); };
$callbacks = ['byVal' => $byVal, 'byRef' => $byRef, 'bump' => $bump, 'strlen' => 'is_null', 'thrower' => $thrower];

function eachElement(string $label, callable $cb): void {
    $rows = [[1, 'two', [3], new Holder, null], ['a', 'b']];
    foreach ($rows as $i => $row) {
        foreach ([0, 1] as $j) {
            try { echo $label, " rows[$i][$j]=", show($cb($rows[$i][$j])), ' -> ', show($rows[$i][$j]), "\n"; }
            catch (RuntimeException $e) { echo $label, " rows[$i][$j] threw ", $e->getMessage(), "\n"; }
        }
    }
    for ($k = 0; $k < 3; $k++) {
        try { echo $label, " for$k ", show($cb($rows[0][$k])), "\n"; }
        catch (RuntimeException $e) { echo $label, " for$k threw\n"; }
    }
}
function eachProperty(string $label, callable $cb): void {
    $box = new Box;
    for ($k = 0; $k < 2; $k++) {
        try { echo $label, " box$k ", show($cb($box->items[0][$k])), ' ', show($cb($box->cell[0][$k])), ' ', show($cb($box->items[0][3]->tag)), ' -> ', show($box->items[0][$k]), ' ', show($box->cell[0][$k]), ' ', show($box->items[0][3]->tag), "\n"; }
        catch (RuntimeException $e) { echo $label, " box$k threw\n"; }
    }
}
function eachArrayAccess(string $label, callable $cb): void {
    $bag = new Bag;
    for ($k = 0; $k < 2; $k++) {
        try { echo $label, " bag$k ", show($cb($bag['k'])), ' ', show($cb($bag['o'])), "\n"; }
        catch (RuntimeException $e) { echo $label, " bag$k threw\n"; }
    }
}
function eachXml(string $label, callable $cb, SimpleXMLElement $x): void {
    for ($k = 0; $k < 2; $k++) {
        try { echo $label, " xml$k ", show($cb($x->c)), ' ', show($cb($x->n)), "\n"; }
        catch (RuntimeException $e) { echo $label, " xml$k threw\n"; }
    }
    echo str_replace("\n", '', $x->asXML()), "\n";
}
// A by-reference callee that WRITES through an array element or a property (`bump`) is the open
// elephc by-reference write-back gap for non-SimpleXML receivers, so it only runs on SimpleXML.
foreach ($callbacks as $label => $cb) {
    $writes = $label === 'bump';
    if (!$writes) { eachElement($label, $cb); eachProperty($label, $cb); }
    if (!$writes && $label !== 'byRef') { eachArrayAccess($label, $cb); }
    eachXml($label, $cb, new SimpleXMLElement('<r><c>old</c><n>5</n></r>'));
}
// Table::render and NamedObjectSet shapes.
final class Table {
    public function render(): void {
        $isCell = static fn ($cell) => $cell instanceof Holder;
        $rows = [];
        foreach ([[1], [new Holder], [null], ['s']] as $i => $row) {
            $rows[$i] = $row;
            echo $isCell($rows[$i][0]) ? 'cell' : 'plain', "\n";
        }
    }
}
(new Table)->render();
