<?php
declare(strict_types=1);
$r = new DemoRecord(7);
try { $r->count = '12'; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->count = 5.0; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->count = true; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->count = 6; var_dump($r->count); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->ratio = 3; var_dump($r->ratio); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->ratio = '2.5'; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $r->ratio = null; var_dump($r->ratio); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
$m = $argc > 5 ? null : $r;
try { $m->count = '13'; echo "done\n"; } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
// The extension's own writes inside its methods are never strict.
try { var_dump($r->describe()); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
