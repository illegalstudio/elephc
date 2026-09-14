---
title: "Operators"
description: "Arithmetic, comparison, logical, bitwise, string, assignment, ternary, null coalescing, and error-control operators."
sidebar:
  order: 2
---

## Arithmetic

| Operator | Example | Notes |
|---|---|---|
| `+` | `$a + $b` | Numeric addition, or PHP array union when both operands are arrays. Integer overflow promotes to `double`. |
| `-` | `$a - $b` | Subtraction. Integer overflow promotes to `double`. |
| `*` | `$a * $b` | Multiplication. Integer overflow promotes to `double`. |
| `/` | `$a / $b` | Division (always returns float). A zero divisor raises a catchable `DivisionByZeroError` ("Division by zero"). |
| `%` | `$a % $b` | Modulo. A zero divisor raises a catchable `DivisionByZeroError` ("Modulo by zero"); `PHP_INT_MIN % -1` is `0`. |
| `**` | `$a ** $b` | Exponentiation (right-associative). Int-preserving like PHP: two `int` operands with a non-negative exponent give an `int` while the result fits (`2 ** 3` is `int(8)`), and promote to `double` at the multiplication that overflows (`2 ** 63`). A negative exponent or a `float` operand always gives a `float`. |
| `-$x` | `-$x` | Unary negation |

### Numeric strings in arithmetic

The arithmetic operators (`+ - * / % **`) accept numeric and leading-numeric strings and coerce
them at runtime, matching PHP 8:

- A pure integer-form string coerces to `int` (`"123" + 3` is `int(126)`).
- A float-form string — one containing a `.` or an exponent — coerces to `float` (`"1.5" + 3` is
  `float(4.5)`, `"1e3" + 1` is `float(1001)`). The runtime routes a float-form string operand
  through the floating-point path, so it is never truncated to an integer first.
- An integer-form string that does not fit a 64-bit integer is a `float` too, exactly as in PHP
  (`"99999999999999999999" + 1` is `float(1.0E+20)`), so the magnitude is preserved instead of
  saturating at `PHP_INT_MAX`. The boundary itself stays an `int`
  (`"9223372036854775807" + 0` is `int(9223372036854775807)`).
- A leading-numeric string uses its numeric prefix and PHP emits `Warning: A non-numeric value
  encountered` (`"  +12foo" + 3` is `int(15)`). For a string **literal** operand elephc reports
  this warning at compile time.
- The classification follows PHP's numeric-string grammar (via the same scan used by
  `is_numeric()`), so hexadecimal (`"0x1A"`), `INF`/`NAN` spellings, and `_` digit separators are
  not recognized as numbers.

Because a string operand's runtime type is not known until it is coerced, the result type of
`+ - * ** %` with a string operand is dynamic (`int` or `float`); `/` stays `float` and `%` stays
`int`, as elsewhere.

A **fully non-numeric** string with no numeric prefix (`"hi"`) is not a valid arithmetic operand:
PHP raises `TypeError: Unsupported operand types`. elephc rejects that at compile time when the
operand is a constant string (`"hi" + 1` is a compile error). See
[Known incompatibilities with PHP](types.md#known-incompatibilities-with-php) for the
runtime-value case.

This coercion is scoped to the arithmetic operators above. Unary negation (`-"5"`) and the
bitwise operators (`"6" << 1`, `&`, `|`, `^`) still require a numeric or integer operand and
reject `string` at compile time.

### Relational comparison between two strings

`<`, `<=`, `>` and `>=` accept **two string operands** and follow PHP's own ordering rule:
when *both* sides are numeric strings they compare numerically, otherwise they compare
byte-wise. That is not the same as `strcmp()`, which is always lexicographic:

```php
var_dump("10" > "9");        // true  — both numeric, so 10 > 9
var_dump("10" < "9a");       // true  — "9a" is not numeric, so bytes: "1" < "9"
var_dump("1e2" == "100");    // true  — exponent notation counts as numeric
var_dump("1.5" <= "1.50");   // true  — numerically equal
var_dump("0x1A" < "26");     // true  — hex is NOT numeric, so bytes: "0" < "2"

$c = "5";
if ($c >= '0' && $c <= '9') { echo "digit\n"; }   // the character-range idiom
```

Mixing a string with a *number* (`$s < 1`) is still rejected at compile time: PHP casts the
number to a string when the string is non-numeric, and elephc does not implement that
conversion — see
[Known incompatibilities with PHP](types.md#known-incompatibilities-with-php).

Two caveats inherited from elephc's numeric-string handling, which apply to `==` just as much
as to the relational operators: an integer string beyond 2^53 loses precision because the
numeric path compares through a double, and a string whose numeric prefix is followed by an
embedded NUL (`"2\0"`) is treated as numeric where PHP treats it as a byte string.

## Comparison

| Operator | Example | Notes |
|---|---|---|
| `==` | `$a == $b` | Loose equality using PHP-style coercions for bool, null, numeric `int`/`float` comparison, numeric strings, non-numeric strings, arrays, and objects |
| `!=` | `$a != $b` | Loose inequality using the same coercions as `==` |
| `<>` | `$a <> $b` | PHP's alias for `!=`: identical semantics, identical precedence and associativity |
| `===` | `$a === $b` | Strict equality (type and value) |
| `!==` | `$a !== $b` | Strict inequality |
| `<` | `$a < $b` | Less than |
| `>` | `$a > $b` | Greater than |
| `<=` | `$a <= $b` | Less than or equal |
| `>=` | `$a >= $b` | Greater than or equal |
| `<=>` | `$a <=> $b` | Spaceship: returns -1, 0, or 1 |
| `instanceof` | `$obj instanceof User` | Runtime class/interface check; returns bool |

`instanceof` supports named class/interface targets plus `self`, `parent`, and `static`. It also supports dynamic targets such as `$obj instanceof $className`, `$obj instanceof $otherObject`, and parenthesized target expressions like `$obj instanceof ($prefix . $suffix)`.

Direct object values and boxed `mixed` / nullable / union values are checked at runtime; scalar, array, and null payloads return `false` after the dynamic target has been validated. Dynamic string targets are matched case-insensitively against class/interface names; unknown class strings return `false`. Dynamic object targets use the target object's runtime class. If a dynamic target is neither a string nor an object, the program exits with a fatal runtime diagnostic.

### Loose equality for arrays and objects

`==` follows PHP 8's comparison table for non-scalar operands as well.

**Arrays.** An array converts to `bool` only against `null` and `bool`; against
anything else it is simply not equal.

```php
<?php
var_dump([] == null);      // true  — both convert to false
var_dump([] == false);     // true
var_dump([0] == true);     // true  — a non-empty array is truthy
var_dump([] == 0);         // false — an array is never equal to a number
var_dump([1] == "1");      // false
var_dump([] == new stdClass()); // false
```

Two arrays are loosely equal when they hold the same number of entries and every
key of the left array exists in the right one with a loosely equal value. Order
does **not** matter (unlike `===`), and a missing key never matches a stored
`null`:

```php
<?php
var_dump(["a" => 1, "b" => 2] == ["b" => 2, "a" => 1]); // true
var_dump([1, 2] == [2 => 1, 3 => 2]);                   // false — different keys
var_dump([1] == ["1"]);                                 // true  — values compare loosely
var_dump(["a" => null] == ["b" => null]);               // false — different keys
```

**Objects.** Two objects are loosely equal when they are the same instance, or
when they share a class and every property compares loosely. `===` keeps meaning
instance identity. Enum cases are singletons, so they compare by identity in both
forms.

```php
<?php
class Point { public int $x = 1; }
$a = new Point();
$b = new Point();
var_dump($a == $b);   // true
var_dump($a === $b);  // false
$b->x = 2;
var_dump($a == $b);   // false
```

Known divergence: PHP raises `Fatal error: Nesting level too deep - recursive
dependency?` when `==` meets a cyclic array/object graph. elephc's comparison
walker stops at a fixed nesting depth and reports "not equal" instead of failing,
so a cyclic comparison terminates normally rather than aborting the program.

`===` and `!==` perform PHP-compatible deep structural comparison for indexed,
associative, nested, and heterogeneous arrays. Keys, values, value types, and
iteration order must all match; associative arrays containing the same pairs in
a different order are therefore not strictly equal.

## Bitwise

| Operator | Example | Notes |
|---|---|---|
| `&` | `$a & $b` | Bitwise AND |
| `\|` | `$a \| $b` | Bitwise OR |
| `^` | `$a ^ $b` | Bitwise XOR |
| `~` | `~$a` | Bitwise NOT |
| `<<` | `$a << $b` | Left shift. A shift count of 64 or more yields `0`; a negative count raises a catchable `ArithmeticError` ("Bit shift by negative number"). |
| `>>` | `$a >> $b` | Arithmetic right shift. A shift count of 64 or more yields `0` for a non-negative value and `-1` for a negative one; a negative count raises a catchable `ArithmeticError`. |

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
@file_get_contents("missing.txt");
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

For `??=` expression form, elephc preserves the PHP short-circuit rule and the conditional write order for non-local targets. If the current target value is non-null, the right-hand side is not evaluated. If it is null, the right-hand side runs before the final write target is evaluated, so forms such as `$items[$i] ??= ($i = 1)` write through the updated simple index while computed/effectful index parts remain stabilized.

## List Unpacking

```php
<?php
[$a, , $c] = [10, 20, 30];
echo $a;  // 10
echo $c;  // 30

list($left, $right) = [1, 2];
echo $left . $right; // 12
```

Destructuring supports skipped entries, nested patterns, associative keys, short
syntax (`[...]`), and `list(...)` syntax. Targets may be local variables or the
same writable non-local forms ordinary assignment supports, such as array
slots, append targets, object properties, and static properties.

```php
<?php
[[$a, $b], [$c, $d]] = [[1, 2], [3, 4]];
["id" => $id, "name" => $name] = ["name" => "Ada", "id" => 7];
[$items[0], $items[]] = [5, 6];
```

As in PHP, keyed and unkeyed entries cannot be mixed in the same
destructuring pattern.

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
echo $user?->profile->address?->city ?? "unknown";
```

`?->` supports nullable object property access and method calls. If the receiver is `null`, elephc skips the rest of the current postfix chain and the expression returns `null`; method arguments, array indexes, and callable arguments on that skipped branch are not evaluated. Mixed chains such as `$user?->profile->address` match PHP: `->address` is skipped when `$user` is `null`, but still warns or fatals normally if `$user` is non-null and `profile` itself is `null`. Use `??` when you want to provide a fallback value.

Nullsafe access cannot be used as an assignment target or combined with first-class callable creation, matching PHP.

## Increment / Decrement

| Operator | Example | Returns |
|---|---|---|
| `++$i` | Pre-increment | New value |
| `$i++` | Post-increment | Old value |
| `--$i` | Pre-decrement | New value |
| `$i--` | Post-decrement | Old value |

In statement position the target can be a local variable, an object property
(including `$this->prop`), an array element, or a static property, in either the
prefix or the postfix spelling:

```php
$this->count++;
++$this->count;
$this->items[0]++;
$obj->count--;
--$obj->items[2];
$totals["a"]++;
```

Statement position discards the operator's result, so `++$x;` and `$x++;` compile to
the same read-modify-write. Reading the result of an increment on a property or array
element — `echo $obj->n++;` — is not supported yet; assign through the statement form
first.

`int`, `float`, `bool`, and `null` values all increment like PHP. Floats add or
subtract exactly `1.0` and stay floats:

```php
$f = 1.5;
$f++;         // float(2.5)
var_dump($f--); // float(2.5) — the post-form returns the old value
var_dump($f);   // float(1.5)
```

`string` values increment with PHP's full rules, including the perl-style alphanumeric
carry:

```php
$s = "az"; $s++;   // string(2) "ba"
$s = "Zz"; $s++;   // string(3) "AAa"
$s = "a9"; $s++;   // string(2) "b0"
$s = "zz"; $s++;   // string(3) "aaa"
$s = "a-"; $s++;   // string(2) "a-"  — the carry stops at the first non-alphanumeric byte
$s = "-a"; $s++;   // string(2) "-b"
$s = "";   $s++;   // string(1) "1"
```

The carry runs over raw bytes from the end of the string: `a`–`y`, `A`–`Y` and `0`–`8`
advance in place, `z`/`Z`/`9` wrap to `a`/`A`/`0` and carry into the previous byte, and
a carry out of the front prepends `a`, `A` or `1`. Any other byte (including the bytes
of a multi-byte character) stops the carry and leaves the rest of the string alone.

A *numeric* string increments as a number instead, so the operator can change the
value's type — which is why a `string` local that is a `++`/`--` target is given boxed
`mixed` storage for its whole lifetime:

```php
$n = "9";    $n++;  // int(10)
$n = "1.5";  $n++;  // float(2.5)
$n = "1e3";  $n++;  // float(1001)
$n = "0x1A"; $n++;  // string(4) "0x1B" — "0x1A" is not a PHP numeric string
```

`--` follows PHP's asymmetric rule: a numeric string decrements numerically, the empty
string becomes `int(-1)`, and any other string is left **unchanged** (`"az"--` is still
`"az"`). PHP additionally raises `E_DEPRECATED` for `++` on a non-alphanumeric string
and for `--` on a non-numeric string; elephc has no runtime deprecation channel, so it
reproduces the resulting value but not the notice.

`++`/`--` on an array, object, buffer, or pointer local stays a compile-time error, as
in PHP where it is a `TypeError`.

## Ternary

```php
$max = $a > $b ? $a : $b;
$label = $name ?: "anonymous";
```

The short ternary / Elvis form `expr ?: fallback` returns the original left-hand value when it is truthy, otherwise it evaluates and returns the fallback. The left-hand expression is evaluated once.

## Pipe (PHP 8.5)

The pipe operator `|>` invokes its right-hand callable with the left-hand value as the single positional argument. `$x |> foo(...)` is equivalent to `foo($x)`. The left-hand side is evaluated before the right-hand side, and chained pipes apply left-to-right.

```php
$result = "hello world"
    |> strtoupper(...)
    |> strrev(...);
// equivalent to: strrev(strtoupper("hello world"))
```

The right-hand side may be any expression that evaluates to a callable:

- First-class callable syntax: `foo(...)`, `Class::method(...)`, `$obj->method(...)`
- A closure literal: `(fn($v) => $v + 1)`
- A variable holding a callable: `$cb`
- Any other expression returning a callable

The callable must accept the piped value as its first parameter; remaining parameters must be optional or variadic. By-reference parameters are not supported on the pipe target.

First-class callable creation accepts local receivers such as `$obj->method(...)` and `$this->method(...)`, plus non-local receiver expressions such as `(new Greeter())->greet(...)` or `(getThing())->method(...)`. The receiver expression is evaluated when the callable is created, then captured in the generated wrapper.

```php
$cb = (new Greeter())->greet(...);
$value |> $cb;
```

Precedence is left-associative and sits below concatenation (`.`), bit shifts, and the additive operators (`+`, `-`). Comparisons, `??`, ternary, logical operators, and assignment all bind looser than `|>`:

```php
echo 5 + 2 |> double(...);          // (5 + 2) |> double(...)
echo "a" . "b" |> wrap(...);        // ("a" . "b") |> wrap(...)
echo "beep" |> strlen(...) == 4;    // (...|> strlen(...)) == 4
echo $id |> get(...) ?? "default";  // (...|> get(...)) ?? "default"
```
