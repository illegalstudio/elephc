<?php

// In PHP, *any* expression can stand alone as a statement — not just assignments and calls.
// A statement may begin with a literal, a comparison, or a unary operator. This enables the
// common "short-circuit guard" idiom `cond && action;` (and `cond || action;`), where the
// right-hand side runs only when the left-hand condition allows it.

// Wrap a signed nibble into 0..63 the way Symfony's intl-normalizer polyfill does:
// `0 > $t && $t += 0x40;` — the `+= 0x40` runs only when $t went negative.
function wrap(int $t): int
{
    0 > $t && $t += 0x40;   // bare expression statement led by the literal `0`
    return $t;
}

echo wrap(-5) . "\n";  // 59  (-5 was negative, so + 64)
echo wrap(10) . "\n";  // 10  (already in range, action skipped)

// `cond || action;` runs the action when the condition is false — a default-assignment guard.
$name = '';
'' === $name && $name = 'anonymous';
echo $name . "\n";     // anonymous

// A bare `new` statement constructs an object purely for its constructor side effect.
class Banner
{
    public function __construct(string $text)
    {
        echo "[$text]\n";
    }
}

new Banner('ready');   // [ready]
