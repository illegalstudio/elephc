<?php

declare(strict_types=1);

// The native run loads only the extension. The source run provides a PHP comparison.
if (!extension_loaded('main')) {
    require __DIR__ . '/main.php';
}

$pippo = new PippoDemo\Pippo(name: 'PHP');
echo $pippo->greet(), "\n";
$pippo->name = 'elephc';
$pippo->options['greeting'] = 'Salve';
echo $pippo->greet(), "\n";
var_dump($pippo->greetings, $pippo->same() === $pippo);
var_dump(PippoDemo\Pippo::twice(21), PippoDemo\Pippo::DEFAULT_NAME, PippoDemo\VERSION);
