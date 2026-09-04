<?php

final class CycleNode
{
    public $next = null;
}

gc_disable();
$node = new CycleNode();
$node->next = $node;
unset($node);

$collected = gc_collect_cycles();
$status = gc_status();

echo "Collected nodes: {$collected}\n";
echo "Productive runs: {$status['runs']}\n";
echo "Collector seconds: {$status['collector_time']}\n";
echo "Destructor seconds: {$status['destructor_time']}\n";
echo "Free seconds: {$status['free_time']}\n";
echo "Live collector candidates: {$status['roots']}\n";
echo "Released allocator cache bytes: ", gc_mem_caches(), "\n";
echo "Automatic GC enabled: ", gc_enabled() ? "yes" : "no", "\n";

gc_enable();
