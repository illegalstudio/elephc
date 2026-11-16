<?php

declare(strict_types=1);

if (!extension_loaded('main')) {
    require __DIR__ . '/main.php';
}

var_dump(native_add(20, 22));
var_dump(native_greet("World\0PHP"));
$original = [2 => "two", "key" => [true, null, 3.5], 9 => false];
var_dump(native_array($original));
var_dump($original);
try {
    native_throw();
} catch (Throwable $exception) {
    echo get_class($exception), ':', $exception->getCode(), ':', $exception->getMessage(), "\n";
}
