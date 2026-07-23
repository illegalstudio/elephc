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
    throw new Error();
} catch (Error $e) {
    echo "caught Error" . PHP_EOL;
}

try {
    $status = 500;
    echo match ($status) {
        200 => "OK",
        default => throw new UnhandledMatchError("unsupported status " . $status),
    };
} catch (\UnhandledMatchError $e) {
    echo "caught UnhandledMatchError: " . $e->getMessage() . PHP_EOL;
}

interface AppThrowable extends Throwable {}

class AppException extends Exception implements AppThrowable {}

try {
    throw new AppException("interface catch", 7);
} catch (Throwable $e) {
    echo "caught Throwable: " . $e->getMessage() . " #" . $e->getCode() . PHP_EOL;
}

try {
    throw new AppException("user interface", 9);
} catch (AppThrowable $e) {
    echo "caught AppThrowable: " . $e->getMessage() . " #" . $e->getCode() . PHP_EOL;
}

try {
    try {
        throw new AppException("root cause", 10);
    } catch (AppException $previous) {
        throw new AppException("wrapped failure", 11, $previous);
    }
} catch (AppException $e) {
    echo $e->getMessage() . " <- " . $e->getPrevious()?->getMessage() . PHP_EOL;
}
