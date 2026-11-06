<?php

// ctype answers byte-class questions without a regular expression. The empty
// string is always false, and every function inspects the whole input.
$fields = [
    'username' => 'alice_42',
    'age' => '42',
    'hex' => 'deadBEEF',
    'control' => "\x01\x1f\x7f",
    'punctuation' => '!@#$%^&*()',
    'printable' => 'hello world',
];

foreach ($fields as $name => $value) {
    printf(
        "%-12s alnum=%d alpha=%d cntrl=%d digit=%d graph=%d lower=%d print=%d punct=%d space=%d upper=%d xdigit=%d\n",
        $name,
        ctype_alnum($value),
        ctype_alpha($value),
        ctype_cntrl($value),
        ctype_digit($value),
        ctype_graph($value),
        ctype_lower($value),
        ctype_print($value),
        ctype_punct($value),
        ctype_space($value),
        ctype_upper($value),
        ctype_xdigit($value),
    );
}

// A common validation pattern: reject anything that is not a decimal integer.
function is_positive_integer(string $value): bool
{
    return $value !== '' && $value[0] !== '-' && ctype_digit($value);
}

foreach (['42', '0', '-7', '3.14', 'abc'] as $candidate) {
    printf("%-6s => %s\n", $candidate, is_positive_integer($candidate) ? 'yes' : 'no');
}
