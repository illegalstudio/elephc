<?php
class RecordKid extends DemoRecord {
    public function kind(): string { return $this->kind; }
    public function initLate(): int { $this->late = 1; return $this->late; }
    public function secret(): mixed { return $this->secret ?? 'unset'; }
}
$r = new DemoRecord(7);
$k = new RecordKid(3);
try { var_dump($r->describe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->count); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->secret); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->kind); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(isset($r->secret)); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(isset($r->kind)); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(isset($r->number)); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->secret = 1; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->kind = 'x'; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { unset($r->kind); echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->number = 8; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->number = 'x'; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->late = 8; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { unset($r->number); echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { unset($r->late); echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->late); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(isset($r->late)); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->count = '12'; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->count); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->count = 'x'; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->count = null; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->count = true; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->count); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->ratio = 3; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->ratio); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->ratio = '2.5'; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->ratio); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->ratio = []; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->ratio = null; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->ratio); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { unset($r->count); echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->count); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->count = 5; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->rotate()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($r->describe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->kind()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->kind); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->initLate()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->secret()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump($k->describe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
