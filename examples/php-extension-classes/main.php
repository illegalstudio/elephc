<?php

namespace PippoDemo;

const VERSION = '1.0';
const DEFAULT_OPTIONS = ['greeting' => 'Ciao'];

#[Export]
final class Pippo
{
    public const DEFAULT_NAME = 'mondo';
    public string $name = self::DEFAULT_NAME;
    public int $greetings = 0;
    public array $options = DEFAULT_OPTIONS;

    public function __construct(string $name = 'mondo')
    {
        $this->name = $name;
    }

    public function greet(): string
    {
        $this->greetings++;
        return $this->options['greeting'] . ', ' . $this->name . '!';
    }

    public function same(): self
    {
        return $this;
    }

    public static function twice(int $value): int
    {
        return $value * 2;
    }
}
