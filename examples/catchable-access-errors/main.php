<?php

// Statically-known access violations that PHP raises as catchable `Error`
// exceptions at runtime, instead of fatal compile-time rejections.

class Vault {
    private function secret(): string {
        return "top-secret";
    }

    protected function guarded(): string {
        return "guarded";
    }
}

class Container {
    public readonly int $value;

    public function __construct(int $value) {
        $this->value = $value;
    }
}

$vault = new Vault();

// Private method access from the global scope throws a catchable Error.
try {
    echo $vault->secret();
    echo "no";
} catch (Error $e) {
    echo "private: " . $e->getMessage() . "\n";
}

// Protected method access from the global scope throws a catchable Error.
try {
    echo $vault->guarded();
    echo "no";
} catch (Error $e) {
    echo "protected: " . $e->getMessage() . "\n";
}

// Readonly property write outside the constructor throws a catchable Error.
$container = new Container(1);
try {
    $container->value = 99;
    echo "no";
} catch (Error $e) {
    echo "readonly: " . $e->getMessage() . "\n";
}

echo "done\n";