<?php
function mk() { return new SimpleXMLElement('<r><c>old</c></r>'); }
function inc(&$v) { $v = $v . '+'; }
function val($v) { $v = $v . '!'; return $v; }
function vinc(&...$vs) { echo count($vs), "|"; }
function vval(...$vs) { foreach ($vs as $x) { $x .= 'w'; } }
class K {
    public static function sinc(&$v) { $v .= 's'; }
    public function minc(&$v) { $v .= 'm'; }
    public function __invoke(&$v) { $v .= 'i'; }
    private function priv(&$v) { $v .= 'p'; }
    public function privCallable(): array { return [$this, 'priv']; }
    public function __call($n, $args) { echo "__call($n)"; return null; }
    public static function __callStatic($n, $args) { echo "__callStatic($n)"; return null; }
}
class V { public function __invoke($v) { $v .= 'x'; } public function plain($v) { $v .= 'y'; } }
function runTyped(string $label, callable $cb): void {
    $a = ['k' => 'a']; $o = new stdClass; $o->p = 'b'; $x = mk();
    $cb($a['k']); $cb($o->p); $cb($x->m);
    echo $label, ':', str_replace("\n", '', $x->asXML()), "\n";
}
function runMixed(string $label, mixed $cb): void {
    $a = ['k' => 'a']; $o = new stdClass; $o->p = 'b'; $x = mk();
    $cb($a['k']); $cb($o->p); $cb($x->m);
    echo $label, ':', str_replace("\n", '', $x->asXML()), "\n";
}
function runNamed(string $label, callable $cb): void {
    $a = ['k' => 'a']; $o = new stdClass; $o->p = 'b'; $x = mk();
    $cb(v: $a['k']); $cb(v: $o->p); $cb(v: $x->m);
    echo $label, ':', str_replace("\n", '', $x->asXML()), "\n";
}
function runVariadic(string $label, callable $cb): void {
    $a = ['k' => 'a']; $o = new stdClass; $o->p = 'b'; $x = mk();
    $cb($o->p, $x->m);
    echo $label, ':', str_replace("\n", '', $x->asXML()), "\n";
}
$k = new K; $vv = new V;
$forms = [
    'closure' => function (&$v) { $v .= 'c'; },
    'closure-val' => function ($v) { $v .= 'c'; },
    'arrow-val' => fn($v) => $v . 'a',
    'fcc' => inc(...), 'fcc-val' => val(...),
    'fcc-static' => K::sinc(...), 'fcc-method' => $k->minc(...),
    'string' => 'inc', 'string-val' => 'val', 'string-static' => 'K::sinc',
    'array-obj' => [$k, 'minc'], 'array-class' => ['K', 'sinc'], 'array-val' => [$vv, 'plain'],
    'invokable' => $k, 'invokable-val' => $vv,
    'private' => $k->privCallable(),
];
foreach ($forms as $label => $cb) { runTyped($label, $cb); runMixed($label, $cb); }
foreach (['closure' => $forms['closure'], 'fcc' => $forms['fcc'], 'string' => 'inc', 'array-obj' => [$k, 'minc'], 'closure-val' => $forms['closure-val']] as $l => $cb) { runNamed($l, $cb); }
runVariadic('variadic-ref', 'vinc'); runVariadic('variadic-val', 'vval');
runVariadic('variadic-closure', function (&...$r) { echo count($r), "|"; });
foreach ([[$k, 'undefinedMethod']] as $f) { runTyped('call-method', $f); }
// Shapes from the Symfony Demo: usort/uasort comparators, Table::render, NamedObjectSet::modify.
$rows = [['n' => 3], ['n' => 1], ['n' => 2]];
usort($rows, fn($l, $r) => $l['n'] <=> $r['n']);
echo json_encode($rows), "\n";
uasort($rows, function ($l, $r) { return $r['n'] <=> $l['n']; });
echo json_encode($rows), "\n";
class NamedSet {
    private array $el = ['a' => 1]; private array $pos = ['q' => 'a'];
    public function modify(string $k, callable $m): void { $p = $this->pos[$k]; echo $k, $p, $m($this->el[$p]), "\n"; }
    public function render(): void {
        $is = static fn ($cell) => $cell instanceof NamedSet; $rows = [];
        foreach ([[1]] as $i => $x) { $rows[$i] = [$x]; echo $is($rows[$i][0]) ? 'y' : 'n', "\n"; }
    }
}
(new NamedSet)->modify('q', fn($x) => $x + 1);
(new NamedSet)->render();
