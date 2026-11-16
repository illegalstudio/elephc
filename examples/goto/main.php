<?php

// goto: jumps to a label in the same function or file.
// elephc rewrites every jump into loops at compile time, so the three idioms
// below cost what the equivalent hand-written loops would.

// 1. Retry: jump back to a label in front of a try block.
function flaky(int $attempt): string
{
    if ($attempt < 3) {
        throw new RuntimeException("attempt $attempt failed");
    }
    return "succeeded on attempt $attempt";
}

function with_retry(): string
{
    $attempt = 0;
    retry:
    try {
        $attempt++;
        return flaky($attempt);
    } catch (RuntimeException $e) {
        echo "  ", $e->getMessage(), "\n";
        if ($attempt < 5) {
            goto retry;
        }
        throw $e;
    }
}

echo "Retry:\n";
$outcome = with_retry();
echo "  ", $outcome, "\n";

// 2. Leave nested loops in one step.
function find_pair(array $numbers, int $target): string
{
    foreach ($numbers as $i => $a) {
        foreach ($numbers as $j => $b) {
            if ($i < $j && $a + $b === $target) {
                goto found;
            }
        }
    }
    return "no pair sums to $target";

    found:
    return "$a + $b = $target";
}

echo "Nested loops:\n";
echo "  ", find_pair([3, 9, 14, 20, 7], 21), "\n";
echo "  ", find_pair([3, 9, 14, 20, 7], 100), "\n";

// 3. A small state machine: labels reached in an order the source does not show.
function tokenize(string $input): array
{
    $tokens = [];
    $i = 0;
    $length = strlen($input);

    start:
    if ($i >= $length) {
        goto done;
    }
    $char = $input[$i];
    if ($char === ' ') {
        $i++;
        goto start;
    }
    if (ctype_digit($char)) {
        goto number;
    }
    goto symbol;

    number:
    $value = '';
    while ($i < $length && ctype_digit($input[$i])) {
        $value .= $input[$i];
        $i++;
    }
    $tokens[] = "num($value)";
    goto start;

    symbol:
    $tokens[] = "sym($char)";
    $i++;
    goto start;

    done:
    return $tokens;
}

echo "State machine:\n";
echo "  ", implode(' ', tokenize('12 + 340 * (7 - 5)')), "\n";

// 4. Cleanup: finally still runs when a jump leaves the try block.
function process(array $jobs): void
{
    foreach ($jobs as $job) {
        try {
            if ($job === 'stop') {
                goto finished;
            }
            echo "  ran $job\n";
        } finally {
            echo "  released $job\n";
        }
    }
    finished:
    echo "  done\n";
}

echo "Cleanup:\n";
process(['build', 'test', 'stop', 'deploy']);
