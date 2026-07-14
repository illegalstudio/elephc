<?php

declare(strict_types=1);

echo "elephc always uses strict typing\n";

declare(ticks=1) {
    echo "braced declare body\n";
}

declare(ticks=1):
    echo "alternative declare body\n";
enddeclare;
