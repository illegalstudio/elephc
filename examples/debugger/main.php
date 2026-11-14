<?php
function greet(string $name, int $times): string {
    $message = str_repeat($name, $times);
    return $message;
}

$label = "hello";
$result = greet($label, 3);
echo $result, "\n";
