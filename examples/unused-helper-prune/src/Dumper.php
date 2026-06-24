<?php

namespace App;

// Referenced only from the unused dump() helper, so it is pruned away with it.
class Dumper
{
    public static function render($value): void
    {
        echo '[dump] ' . var_export($value, true) . "\n";
    }
}
