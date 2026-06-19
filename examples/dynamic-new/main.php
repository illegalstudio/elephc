<?php

// Dynamic object construction where the class name is not a literal but the value
// of an expression: an array element, an object property, or a parenthesized call.
// This is the "service registry" / factory pattern that frameworks such as Symfony
// use to bootstrap (`new $_SERVER['APP_RUNTIME'](...)`).

class JsonRenderer
{
    public function __construct(public string $label) {}

    public function render(array $data): string
    {
        return $this->label . ': {' . implode(', ', $data) . '}';
    }
}

class CsvRenderer
{
    public function __construct(public string $label) {}

    public function render(array $data): string
    {
        return $this->label . ': ' . implode(',', $data);
    }
}

// 1) Class name held in an array element — the canonical registry lookup.
$registry = [
    'json' => 'JsonRenderer',
    'csv'  => 'CsvRenderer',
];

$format = 'json';
$renderer = new $registry[$format]('output');
echo $renderer->render(['a', 'b', 'c']), "\n";

// 2) Class name read from an object property.
class RendererConfig
{
    public string $default = 'CsvRenderer';
}

$config = new RendererConfig();
$fallback = new $config->default('fallback');
echo $fallback->render(['x', 'y']), "\n";

// 3) PHP 8.0 parenthesized expression naming the class.
function chooseRenderer(): string
{
    return 'JsonRenderer';
}

$chosen = new (chooseRenderer())('chosen');
echo $chosen->render(['1', '2']), "\n";
