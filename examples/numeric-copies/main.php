<?php
// Arithmetic assignments preserve independent value copies and reference aliases.
function showCopies(int $seed): void
{
    $value = $seed + 10;
    $copy = $value;
    $value += 2;
    $reference =& $value;
    $reference += 3;
    echo "Updated: ", $value, "; copied: ", $copy, "\n";
}

showCopies($argc);
