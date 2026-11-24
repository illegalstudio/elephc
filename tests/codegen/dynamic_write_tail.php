class Mg { public array $log = []; public $declared = 'd'; public function __set($n, $v) { $this->log[] = "set:$n"; if ($n === 'chain') { $this->other = $v; } if ($n === 'same') { $this->same = $v; } if ($n === 'boom') { throw new RuntimeException("boom"); } } }
class MgChild extends Mg {}
#[AllowDynamicProperties] class Pl { public $a = 1; protected $prot = 2; public int $typed = 0; }
class Strict { public $a = 1; }
function pick($i) { $k = ['Mg', 'MgChild', 'Pl', 'Strict'][$i]; return new $k(); }
function setName($o, $n, $v) { $o->$n = $v; }
function setLit($o, $v) { $o->fresh = $v; $o->{'chain'} = $v; }
function t($f) { try { $f(); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), ' @', $e->getLine(), "\n"; } }
set_error_handler(function ($no, $str, $file, $line) { echo "ERR[$no] " . json_encode($str) . " @$line\n"; return true; });
for ($c = 0; $c < 4; $c++) {
    foreach (['declared', 'a', 'chain', 'same', 'new', '0', 'typed', 'prot', 'boom', "x\0y"] as $name) {
        foreach ([5, 'str', [1], null] as $v) {
            $o = pick($c);
            echo get_class($o), ' ', json_encode($name), ' ', json_encode($v), ': ';
            t(function () use ($o, $name, $v) { setName($o, $name, $v); });
            echo $o instanceof Mg ? json_encode($o->log) : '', ' ', count((array) $o), "\n";
        }
    }
    $o = pick($c);
    t(function () use ($o) { setLit($o, 7); });
    echo get_class($o), ' ', count((array) $o), $o instanceof Mg ? ' ' . json_encode($o->log) : '', "\n";
}
echo "done\n";
