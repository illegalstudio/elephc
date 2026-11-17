<?php
function observeSuspendedScope(): Generator {
    eval('$value = 1;');
    yield 1;
    try { eval('$value = "bad";'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
}
foreach (observeSuspendedScope() as $step) { echo $step; gc_collect_cycles(); }
