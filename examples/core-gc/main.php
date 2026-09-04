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
echo "Automatic GC enabled: ", gc_enabled() ? "yes" : "no", "\n";

gc_enable();
