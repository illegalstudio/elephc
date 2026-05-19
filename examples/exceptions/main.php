<?php

class DivisionByZeroException extends Exception {}

function safe_divide($left, $right) {
    if ($right == 0) {
        throw new DivisionByZeroException();
    }
    return intdiv($left, $right);
}

try {
    echo safe_divide(10, 2) . PHP_EOL;
    echo safe_divide(10, 0) . PHP_EOL;
} catch (DivisionByZeroException $e) {
    echo "caught divide by zero" . PHP_EOL;
} finally {
    echo "cleanup complete" . PHP_EOL;
}

try {
    throw new Error("core error");
} catch (Error $e) {
    echo "caught Error: " . $e->getMessage() . PHP_EOL;
}
