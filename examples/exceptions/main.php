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

// A domain exception with its own constructor forwarding to the builtin parent:
// parent::__construct() stamps the inherited message/code, and the subclass adds a
// field of its own — the common framework pattern for rich exceptions.
class HttpException extends RuntimeException {
    public string $url = "";

    public function __construct(string $message, int $status, string $url) {
        parent::__construct($message, $status);
        $this->url = $url;
    }
}

try {
    throw new HttpException("not found", 404, "/missing");
} catch (HttpException $e) {
    echo "caught HttpException: " . $e->getMessage()
        . " #" . $e->getCode() . " (" . $e->url . ")" . PHP_EOL;
}
