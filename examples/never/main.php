<?php

class ConfigError extends Exception {}

function panic(string $message): never {
    throw new ConfigError($message);
}

function require_positive(string $name, int $value): int {
    if ($value <= 0) {
        panic("config error: " . $name . " must be positive, got " . $value);
    }
    return $value;
}

try {
    $port = require_positive("port", 8080);
    echo "port = " . $port . PHP_EOL;

    $workers = require_positive("workers", 0);
    echo "unreachable" . PHP_EOL;
} catch (ConfigError $e) {
    echo "caught: " . $e->getMessage() . PHP_EOL;
}
