<?php
// Sieve-style prime checker using all language features

function is_prime($n) {
    if ($n <= 1) {
        return false;
    }
    if ($n <= 3) {
        return true;
    }
    if ($n % 2 == 0 || $n % 3 == 0) {
        return false;
    }
    $i = 5;
    while ($i * $i <= $n) {
        if ($n % $i == 0 || $n % ($i + 2) == 0) {
            return false;
        }
        $i += 6;
    }
    return true;
}

echo "Primes up to 50:\n";
$count = 0;
for ($n = 2; $n <= 50; $n++) {
    if (is_prime($n)) {
        echo $n . " ";
        $count++;
    }
}
echo "\n" . $count . " primes found\n";
