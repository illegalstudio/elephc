<?php

// Default parameter values and heredoc strings

function greet($name = "world", $greeting = "Hello") {
    echo $greeting . " " . $name . "!\n";
}

greet();                    // Hello world!
greet("PHP");               // Hello PHP!
greet("elephc", "Hola");    // Hola elephc!

function power($base, $exp = 2) {
    $result = 1;
    for ($i = 0; $i < $exp; $i++) {
        $result = $result * $base;
    }
    return $result;
}

echo power(5) . "\n";      // 25
echo power(2, 10) . "\n";  // 1024

// Heredoc string
echo <<<EOT
This is a multi-line
heredoc string.
It supports escape sequences like tabs:	done.
EOT;
echo "\n";

// Nowdoc string (no escape processing)
echo <<<'EOT'
This is a nowdoc string.
Backslash-n stays literal: \n
EOT;
echo "\n";
