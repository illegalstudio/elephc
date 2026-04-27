---
title: "Operators"
description: "Arithmetic, comparison, logical, bitwise, string, and assignment operators."
sidebar:
  order: 2
---

## Arithmetic

| Operator | Example | Notes |
|---|---|---|
| `+` | `$a + $b` | Addition |
| `-` | `$a - $b` | Subtraction |
| `*` | `$a * $b` | Multiplication |
| `/` | `$a / $b` | Division (always returns float) |
| `%` | `$a % $b` | Modulo |
| `**` | `$a ** $b` | Exponentiation (right-associative, returns float) |
| `-$x` | `-$x` | Unary negation |

## Comparison

| Operator | Example | Notes |
|---|---|---|
| `==` | `$a == $b` | Loose equality (cross-type: coerces to int) |
| `!=` | `$a != $b` | Inequality |
| `===` | `$a === $b` | Strict equality (type and value) |
| `!==` | `$a !== $b` | Strict inequality |
| `<` | `$a < $b` | Less than |
| `>` | `$a > $b` | Greater than |
| `<=` | `$a <= $b` | Less than or equal |
| `>=` | `$a >= $b` | Greater than or equal |
| `<=>` | `$a <=> $b` | Spaceship: returns -1, 0, or 1 |
| `instanceof` | `$obj instanceof User` | Runtime class/interface check; returns bool |

`instanceof` supports named class/interface targets plus `self`, `parent`, and `static`. Direct object values and boxed `mixed` / nullable / union values are checked at runtime; scalar, array, and null payloads return `false`. Unknown class/interface targets return `false`, matching PHP. Dynamic RHS targets such as `$obj instanceof $className` are not supported yet and are tracked in `ROADMAP.md`.

## Bitwise

| Operator | Example | Notes |
|---|---|---|
| `&` | `$a & $b` | Bitwise AND |
| `\|` | `$a \| $b` | Bitwise OR |
| `^` | `$a ^ $b` | Bitwise XOR |
| `~` | `~$a` | Bitwise NOT |
| `<<` | `$a << $b` | Left shift |
| `>>` | `$a >> $b` | Arithmetic right shift |

## Logical

| Operator | Example | Notes |
|---|---|---|
| `&&` | `$a && $b` | AND with short-circuit; higher precedence than `and` |
| `\|\|` | `$a \|\| $b` | OR with short-circuit; higher precedence than `or` |
| `and` | `$a and $b` | Word-form AND with short-circuit; lower precedence than `?:` and `??` |
| `or` | `$a or $b` | Word-form OR with short-circuit; lower precedence than `xor` and `and` |
| `xor` | `$a xor $b` | Word-form exclusive OR; evaluates both operands |
| `!` | `!$a` | NOT |

Word-form logical precedence matches PHP: `and` binds tighter than `xor`, and `xor` binds tighter than `or`. All three bind looser than `&&`, `||`, `??`, and the ternary operators.

Word-form logical operators are case-insensitive (`AND`, `Or`, and `xOr` are accepted). Assignment expressions are not part of elephc's expression subset yet, so use parentheses when a word-form logical expression is the right-hand side of an assignment: `$x = (true and false);`.

## String

| Operator | Example | Notes |
|---|---|---|
| `.` | `"a" . "b"` | Concatenation |
| `.` | `"val=" . 42` | Auto-coerces int to string |
| `.` | `"pi=" . 3.14` | Auto-coerces float to string |

## Assignment

| Operator | Example | Equivalent |
|---|---|---|
| `=` | `$x = 5` | Simple assignment |
| `+=` | `$x += 5` | `$x = $x + 5` |
| `-=` | `$x -= 5` | `$x = $x - 5` |
| `*=` | `$x *= 5` | `$x = $x * 5` |
| `**=` | `$x **= 5` | `$x = $x ** 5` |
| `/=` | `$x /= 5` | `$x = $x / 5` |
| `%=` | `$x %= 5` | `$x = $x % 5` |
| `.=` | `$s .= "x"` | `$s = $s . "x"` |
| `&=` | `$x &= 5` | `$x = $x & 5` |
| `|=` | `$x |= 5` | `$x = $x | 5` |
| `^=` | `$x ^= 5` | `$x = $x ^ 5` |
| `<<=` | `$x <<= 2` | `$x = $x << 2` |
| `>>=` | `$x >>= 2` | `$x = $x >> 2` |
| `??=` | `$x ??= "default"` | Assign RHS only when `$x` is `null` |

Compound assignments are supported for local variable assignments and `for` init/update clauses.

## List Unpacking

```php
<?php
[$a, $b, $c] = [10, 20, 30];
echo $a;  // 10
```

**Limitations:** All elements must be variables (no nested patterns or skipping). RHS must be an indexed array.

## Null Coalescing

```php
$x = null;
echo $x ?? "default";    // prints "default"
echo $x ?? $y ?? "last"; // chained — right-associative

$x ??= "fallback";       // assigns because $x is null
$x ??= "ignored";        // keeps "fallback"; RHS is not evaluated
```

`??=` is supported for already-declared local/global/static variables as a standalone assignment statement.
For concrete local variable types, the fallback must keep the same static type, or be a literal `null`.
Use a nullable, union, or `mixed` typed local when the fallback may change the stored runtime representation.

## Increment / Decrement

| Operator | Example | Returns |
|---|---|---|
| `++$i` | Pre-increment | New value |
| `$i++` | Post-increment | Old value |
| `--$i` | Pre-decrement | New value |
| `$i--` | Post-decrement | Old value |

## Ternary

```php
$max = $a > $b ? $a : $b;
$label = $name ?: "anonymous";
```

The short ternary / Elvis form `expr ?: fallback` returns the original left-hand value when it is truthy, otherwise it evaluates and returns the fallback. The left-hand expression is evaluated once.
