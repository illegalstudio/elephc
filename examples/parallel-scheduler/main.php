<?php

use Elephc\Parallel\TaskGroup;
use function Elephc\Parallel\run;

function parallel_sum(array $values): int
{
    return $values[0] + $values[1];
}

$prefix = "parallel";
$result = run(static function (TaskGroup $tasks) use ($prefix): array {
    $left = $tasks->spawn(
        static fn (int $value): string => $prefix . ":" . ($value * 2),
        21,
    );
    $right = $tasks->spawn(parallel_sum(...), [20, 22]);
    $length = $tasks->spawn(strlen(...), "elephc");

    return [$left->join(), $right->join(), $length->join()];
});

echo $result[0], "|", $result[1], "|", $result[2], PHP_EOL;
