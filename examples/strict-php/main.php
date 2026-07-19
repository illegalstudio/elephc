<?php
// Compile with: elephc --strict-php examples/strict-php/main.php
//
// Under --strict-php the compiler accepts only PHP-compatible constructs:
// elephc extensions (ifdef, packed class, extern, ptr/buffer, extension
// builtins) become compile errors, so everything below also runs unchanged
// under the PHP interpreter: php examples/strict-php/main.php
//
// Extension builtins do not exist in strict mode, exactly like in PHP —
// a program may even declare its own function using one of those names.

function ptr_get(array $slots, int $index): string
{
    return $slots[$index] ?? '<none>';
}

$slots = ['alpha', 'beta', 'gamma'];

echo "ptr_get is ours: ", ptr_get($slots, 1), "\n";
echo "function_exists('ptr_get'): ", var_export(function_exists('ptr_get'), true), "\n";
echo "function_exists('zval_pack'): ", var_export(function_exists('zval_pack'), true), "\n";

// Plain PHP keeps working as usual.
$total = 0;
foreach ($slots as $name) {
    $total += strlen($name);
}
echo "total name length: {$total}\n";
