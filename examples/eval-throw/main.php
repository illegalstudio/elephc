<?php

eval('class DynamicRequestException extends Exception {}');

$failure = new DynamicRequestException("closed");
try {
    throw $failure;
} catch (Exception $error) {
    echo "statement: " . $error->getMessage() . "\n";
}

try {
    $result = true ? throw new DynamicRequestException("missing field") : "ok";
    echo $result . "\n";
} catch (Exception $error) {
    echo "expression: " . $error->getMessage() . "\n";
}
