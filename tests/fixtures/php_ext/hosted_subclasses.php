<?php
class SecretKid extends DemoRecord {
    public function write() { $this->secret = 1; return $this->secret; }
    public function probe() { return isset($this->secret); }
    public function drop() { unset($this->secret); return isset($this->secret); }
}
class LooseAccessors extends DemoRecord {
    public function __get($name) { return "get:$name"; }
    public function __set($name, $value) { echo "set $name\n"; }
    public function __isset($name) { return $name === 'yes'; }
    public function __unset($name) { echo "unset $name\n"; }
    public function __clone() { echo "clone ", $this->describe(), "\n"; }
}
class Routed extends DemoRecord {
    public function __get($n) { return "get:$n"; }
    public function __set($n, $v) { echo "set $n\n"; }
    public function __isset($n) { echo "isset $n\n"; return true; }
    public function inner() { return $this->kind; }
}
class KindReader extends DemoRecord { public function kind() { return $this->kind; } }
class KindReaderChild extends KindReader {}
$k = new SecretKid(3);
try { var_dump($k->probe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->write()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->probe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->describe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->drop()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->describe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
$l = new LooseAccessors(4);
try { var_dump($l->kind); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $l->q = 1; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(isset($l->yes), isset($l->no)); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { unset($l->q); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $c = clone $l; var_dump($c->describe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
$r = $argc > 5 ? new KindReader(1) : new KindReaderChild(2);
try { var_dump($r->kind()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump((new KindReaderChild(3))->kind()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump((new DemoRecord(5))->kind); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
$o = new Routed(4);
try { var_dump($o->count, $o->kind, $o->secret, $o->zzz, $o->number); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $o->count = 9; var_dump($o->count); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $o->kind = 'x'; $o->q = 1; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(isset($o->count), isset($o->kind), isset($o->zz)); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($o->inner()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $o->number = 5; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
