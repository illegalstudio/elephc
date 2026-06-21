<?php

// `goto` jumps to a labelled statement in the same function. PHP restricts it to jumping out of or
// within a block (you cannot jump *into* a loop or switch), which makes it handy for two things:
// breaking out of deeply nested loops, and skipping ahead to shared cleanup/recovery code.

// 1. Break out of nested loops in one jump.
// `break N` works too, but `goto` reads clearly when the target is a specific labelled point.
$needle = 42;
$grid = [[1, 2, 3], [4, 42, 6], [7, 8, 9]];
$found = "not found";
foreach ($grid as $row => $cells) {
    foreach ($cells as $col => $value) {
        if ($value === $needle) {
            $found = "found $needle at row $row, col $col";
            goto done_searching;
        }
    }
}
done_searching:
echo $found, "\n";

// 2. Skip ahead to shared recovery code from inside a try/catch — the pattern Twig's
// CoreExtension::getAttribute uses. On failure we normalise the input and continue at `method_check`.
echo describe(null), "\n";
echo describe("widget"), "\n";

function describe($thing): string
{
    if ($thing === null) {
        try {
            throw new InvalidArgumentException("missing value");
        } catch (InvalidArgumentException $e) {
            // Recover by substituting a default, then jump to the common path.
            $thing = "default";
            goto method_check;
        }
    }

    method_check:
    return "describing: " . $thing;
}

// 3. A simple retry loop expressed with a backward goto.
$attempt = 0;
retry:
$attempt++;
echo "attempt $attempt\n";
if ($attempt < 3) {
    goto retry;
}
echo "succeeded after $attempt attempts\n";
