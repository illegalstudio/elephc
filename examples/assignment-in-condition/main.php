<?php

// In PHP an assignment `=` binds to the lvalue immediately to its left, even inside a
// comparison or other higher-precedence operator. So `false !== $pos = strrpos(...)` runs
// the assignment first and compares its result — the idiom Composer's class loader uses to
// split a namespaced class name into its namespace and short name.

function shortName(string $class): string
{
    // Assign-and-test: `$pos` captures the last separator position, and the comparison
    // decides whether the class is namespaced.
    if (false !== $pos = strrpos($class, '\\')) {
        return substr($class, $pos + 1);
    }
    return $class;
}

echo shortName('App\\Service\\Mailer') . "\n"; // Mailer
echo shortName('Stringable') . "\n";           // Stringable (no namespace)

// The same binding applies under arithmetic and the prefix `!`.
$n = 0;
echo "1 + (\$n = 5) = " . (1 + $n = 5) . ", \$n is now " . $n . "\n";

$flag = true;
$result = !$flag = false;   // parses as !($flag = false): assigns false, negates to true
echo "!(\$flag = false) = " . ($result ? "true" : "false")
    . ", \$flag is now " . ($flag ? "true" : "false") . "\n";
