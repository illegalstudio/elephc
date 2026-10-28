<?php
class EvalKid extends DemoRecord {
    public function kind(): mixed { return eval('return $this->kind;'); }
    public function hasKind(): mixed { return eval('return isset($this->kind);'); }
    public function setKind(): mixed { eval('$this->kind = "evk";'); return $this->kind; }
}
$k = new EvalKid(5);
try { var_dump($k->kind()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->hasKind()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->setKind()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(eval('return $k->kind;')); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(eval('return $k->count;')); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
