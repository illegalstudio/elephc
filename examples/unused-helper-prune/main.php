<?php

// Uses App\Greeter but never calls the optional dump() helper, so App\Dumper is pruned from the
// binary while App\Greeter is compiled in normally.

use App\Greeter;

$g = new Greeter();
echo $g->hello('world') . "\n";
echo "done\n";
