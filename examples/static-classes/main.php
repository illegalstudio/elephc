<?php

namespace App\Demo;

// `Class::class` returns the fully-qualified name of the class as a string.
// Useful for logging, registration tables, and tagged unions.
echo "Logger class FQN: " . Logger::class . "\n";

class Logger {
    public string $tag;

    public function __construct(string $tag) {
        $this->tag = $tag;
    }

    // `self::class` is resolved at compile time to the FQN of this class —
    // safer than hardcoding the name as a string (renames just work).
    public function describe(): string {
        return self::class . "[" . $this->tag . "]";
    }

    // `new self()` is the canonical factory pattern. The compiler resolves it
    // to `new App\Demo\Logger()` at compile time.
    public static function default_logger(): Logger {
        return new self("default");
    }
}

$default = Logger::default_logger();
echo "Default logger: " . $default->describe() . "\n";

$named = new Logger("audit");
echo "Named logger: " . $named->describe() . "\n";

// Inheritance + parent::class
class Base {
    public string $kind = "base";
}

class Specialized extends Base {
    // `parent::class` returns the FQN of the parent class.
    public static function parent_name(): string {
        return parent::class;
    }

    // `new parent()` constructs an instance of the parent class.
    public static function make_base(): Base {
        return new parent();
    }
}

echo "Specialized's parent is: " . Specialized::parent_name() . "\n";
$b = Specialized::make_base();
echo "Constructed base, kind=" . $b->kind . "\n";
