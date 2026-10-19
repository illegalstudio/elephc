<?php

// Variable variables: `$$name` reads or writes the local whose name is the runtime value of
// `$name`. elephc dispatches over the locals the function spells at compile time.

function describe_interval($years = 0, $months = 0, $days = 0): string
{
    $parts = [];
    foreach (['years', 'months', 'days'] as $unit) {
        $value = $$unit;
        if ($value) {
            $parts[] = "$value $unit";
        }
    }
    return implode(', ', $parts);
}

function bump(string $unit, int $by): string
{
    [$hours, $minutes] = [1, 30];
    switch ($unit) {
        case 'hours':
        case 'minutes':
            $$unit += $by;
            break;
    }
    return sprintf('%02d:%02d', $hours, $minutes);
}

function where_clause($column, $value, $boolean = 'and'): array
{
    $type = 'Basic';
    return compact('type', 'column', 'value', 'boolean');
}

function dsn(array $config): string
{
    extract($config, EXTR_SKIP);
    return 'pgsql:host=' . ($host ?? 'localhost') . ';port=' . ($port ?? 5432);
}

echo describe_interval(1, 0, 3), "\n";
echo bump('minutes', 15), ' ', bump('hours', 2), "\n";
echo json_encode(where_clause('email', 'a@b.c')), "\n";
echo dsn(['host' => 'db']), "\n";
