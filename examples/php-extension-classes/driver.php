<?php

declare(strict_types=1);

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
