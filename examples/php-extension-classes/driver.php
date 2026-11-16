<?php

declare(strict_types=1);

$limitText = $argv[1] ?? '2000000';
$limit = (int)$limitText;
if (count($argv) > 2 || $limit < 2 || (string)$limit !== $limitText) {
    fwrite(STDERR, "Usage: php [PHP options] driver.php [integer limit >= 2]\n");
    exit(1);
}

// Show which implementation runs before the shared demonstration below.
if (extension_loaded('main')) {
    echo "Mode: native extension (main.so)\n";
} else {
    echo "Mode: PHP source (fallback)\n";
    require __DIR__ . '/main.php';
}

$pippo = new PippoDemo\Pippo(name: 'PHP');
echo $pippo->greet(), "\n";
$pippo->name = 'elephc';
$pippo->options['greeting'] = 'Salve';
echo $pippo->greet(), "\n";
var_dump($pippo->greetings, $pippo->same() === $pippo);
var_dump(PippoDemo\Pippo::twice(21), PippoDemo\Pippo::DEFAULT_NAME, PippoDemo\VERSION);

// Measure one complete method call, including the native adapter boundary when loaded.
$started = hrtime(true);
$primes = PippoDemo\Pippo::countPrimes($limit);
$seconds = (hrtime(true) - $started) / 1000000000;
printf("Benchmark: count primes up to %d\nPrimes: %d\nElapsed: %.6f s\n", $limit, $primes, $seconds);
