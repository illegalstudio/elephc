---
title: "Operators"
description: "Arithmetic, comparison, logical, bitwise, string, assignment, ternary, null coalescing, and error-control operators."
sidebar:
  order: 2
---

## Arithmetic

| Operator | Example | Notes |
|---|---|---|
| `+` | `$a + $b` | Numeric addition, or PHP array union when both operands are arrays |
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

Word-form logical operators are case-insensitive (`AND`, `Or`, and `xOr` are accepted). Assignment expressions bind tighter than `and`, `xor`, and `or`, matching PHP: `$x = true and false` is parsed as `($x = true) and false`.

## Error Control

PHP's error-control operator `@` suppresses runtime warnings for exactly one expression.

```php
<?php
$value = @file_get_contents("missing.txt");
echo "still running";
```

The operand is evaluated normally and its value is preserved. Only suppressible runtime warnings are hidden; compile-time errors and fatal runtime errors still report normally. Nested `@` scopes are tracked with a runtime suppression depth, and exception unwinds restore the previous suppression state before entering a `catch`.

## Print Expression

`print` is supported as a PHP-compatible expression: it writes its operand to
stdout and returns `1`.

```php
<?php
$status = print "ready\n";
echo $status;          // 1
echo print "nested";   // nested1
```

Its precedence matches PHP: the operand can include `?:` and `??`, while the
word-form logical operators `and`, `xor`, and `or` bind looser than the whole
`print` expression.

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

Compound assignments are supported for local variable assignments, `for` init/update clauses, object properties, static properties, indexed array elements, property-backed indexed array elements, and static-property-backed indexed array elements:

```php
$items[0] += 3;
$box->count *= 2;
$box->items[1] >>= 1;
Counter::$count **= 2;
Registry::$items[0] ??= 10;
```

Append targets such as `$items[] += 1` are invalid; append syntax is only supported with plain assignment (`$items[] = 1`).
Receiver and index expressions are evaluated once for non-local compound targets, matching PHP read-modify-write behavior for forms such as `$items[f()] += 1` and `getBox()->count += 1`.

Local variable assignments can also be used as expressions. The expression returns the assigned value and still updates the local:

```php
<?php
echo ($x = 5);      // 5
echo $x;            // 5

$x = true and false;
echo $x ? "T" : "F"; // T

$count = 4;
echo ($count += 3); // 7
```

Assignment expression precedence matches PHP: assignment binds lower than `?:` and `??`, but higher than the word-form logical operators. The expression form supports local variables plus stabilized non-local targets such as array elements, object properties, static properties, and indexed array slots stored in properties:

```php
<?php
$items = [1, 2];
echo ($items[1] = 5); // 5

class Box {
    public $count = 1;
    public $items = [2];
}
$box = new Box();
echo ($box->count += 4);
echo ($box->items[0] *= 3);

class Registry {
    public static ?int $value = null;
}
echo (Registry::$value ??= 10);
```

Non-local assignment expression targets stabilize receiver and index subexpressions when needed, so side-effecting targets such as `$items[idx()] = 1` and `make_box()->count += 1` are evaluated once. For `=` and compound assignment, elephc also preserves PHP's ordering for RHS-mutated simple indexes such as `$items[$i] = ($i = 1)` while pre-evaluating computed indexes such as `$items[$i + 0] = ($i = 1)`.

**Current limitation:** `??=` expression form still rejects non-local targets whose RHS mutates a variable used by the target, such as `$items[$i] ??= ($i = 1)`. This keeps the RHS short-circuit behavior correct until conditional non-local write stabilization is implemented.

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

`??=` is supported for already-declared local/global/static variables and non-append property/array/static-property assignment targets as a standalone assignment statement.
For concrete local variable types, the fallback must keep the same static type, or be a literal `null`.
Use a nullable, union, or `mixed` typed local when the fallback may change the stored runtime representation.

## Nullsafe Access

```php
<?php
echo $user?->profile?->name ?? "anonymous";
echo $user?->profile?->label() ?? "missing";
```

`?->` supports nullable object property access and method calls. If the receiver is `null`, the member access or method call is skipped and the expression returns `null`; method arguments are not evaluated on that skipped branch. Use `??` when you want to provide a fallback value.

Nullsafe access cannot be used as an assignment target or combined with first-class callable creation, matching PHP. Full PHP parity for mixed chains such as `$a?->b->c` is tracked in `ROADMAP.md`; write each nullable hop explicitly as `?->` today.

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
