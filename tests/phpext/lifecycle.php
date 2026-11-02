<?php

#[Export]
function phpext_counter(): int
{
    static $counter = 0;
    ++$counter;
    return $counter;
}

#[Export]
function phpext_label(string $value): string
{
    static $label = 'label:';
    return $label . $value;
}

#[Export]
function phpext_mirror(array $value): array
{
    return $value;
}

#[Export]
function phpext_failure(): void
{
    throw new RuntimeException('test failure', 42);
}

#[Export]
function phpext_recurse(int $depth): int
{
    return $depth > 0 ? phpext_recurse($depth - 1) + 1 : 0;
}

#[Export]
function phpext_output(string $value): string
{
    echo $value;
    return $value;
}

#[Export]
function phpext_pause(string $message): void
{
    $values = [];
    $missing = $values['missing'];
    if ($missing !== null) { throw new RuntimeException('unexpected array value'); }
    throw new RuntimeException($message, 42);
}

#[Export]
final class LifecycleObject
{
    public int $value = 0;
    public ?LifecycleObject $next = null;
    public array $items = [];

    public function add(int $amount): int
    {
        $this->value += $amount;
        return $this->value;
    }

    public function pause(string $message): void
    {
        $this->value++;
        phpext_pause($message);
    }
}

#[Export]
function phpext_object_kept(?LifecycleObject $object, bool $set): ?LifecycleObject
{
    static $saved = null;
    if ($set) { $saved = $object; }
    return $saved;
}

#[Export]
function phpext_object_churn(int $count): void
{
    for ($i = 0; $i < $count; $i++) {
        $object = new LifecycleObject();
        phpext_object_kept($object, true);
        echo 'object';
        phpext_object_kept(null, true);
    }
}
