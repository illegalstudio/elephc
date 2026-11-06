<?php
// Choose the source at runtime so eval executes through Magician.
$declarations = $argc > 1 ? $argv[1] : <<<'CODE'
function identity<T>(T $value): T { return $value; }

class Box<T>
{
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
    public function map<U>(callable(T): U $transform): Box<U>
    {
        return new Box<U>($transform($this->value));
    }
}
CODE;
eval($declarations);

// Templates and their concrete specializations survive subsequent eval calls.
$use = $argc > 1 ? '' : <<<'CODE'
$box = new Box('Magician');
$length = $box->map(function(string $text): int { return strlen($text); });
echo identity(42), '|', $box->get(), '|', $length->get(), "\n";
try {
    identity<int>(['wrong']);
} catch (TypeError $error) {
    echo "type checked\n";
}
CODE;
eval($use);
