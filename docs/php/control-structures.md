---
title: "Control Structures"
description: "if/else, while, for, foreach, switch, match, try/catch, and more."
sidebar:
  order: 3
---

## if / elseif / else

```php
<?php
if ($x > 0) {
    echo "positive";
} elseif ($x < 0) {
    echo "negative";
} else {
    echo "zero";
}
```

## while

```php
<?php
$i = 0;
while ($i < 10) {
    echo $i;
    $i++;
}
```

## do...while

```php
<?php
$i = 0;
do {
    $i++;
} while ($i < 10);
```

## for

```php
<?php
for ($i = 0; $i < 10; $i++) {
    echo $i;
}
```

## foreach

```php
<?php
$arr = [1, 2, 3];
foreach ($arr as $value) {
    echo $value . "\n";
}

// With key binding (indexed arrays)
foreach ($arr as $i => $value) {
    echo "$i: $value\n";
}

// With key binding (associative arrays)
$map = ["name" => "Alice", "age" => "30"];
foreach ($map as $key => $value) {
    echo "$key = $value\n";
}
```

## break / continue

```php
<?php
for ($i = 0; $i < 100; $i++) {
    if ($i == 5) { break; }
    if ($i % 2 == 0) { continue; }
    echo $i . " ";
}
// Output: 1 3
```

**Not supported:** `break 2;` / `continue 2;` (multi-level).

## switch / case / default

Standard PHP switch with fall-through semantics. Use `break` to prevent fall-through.

```php
<?php
$x = 2;
switch ($x) {
    case 1:
        echo "one";
        break;
    case 2:
        echo "two";
        break;
    default:
        echo "other";
        break;
}
```

Fall-through example:

```php
<?php
$x = 1;
switch ($x) {
    case 1:
    case 2:
        echo "one or two";
        break;
    default:
        echo "other";
}
```

## match expression

PHP 8 style match. No fall-through, returns a value, uses strict comparison (`===`).

```php
<?php
$x = 2;
$result = match($x) {
    1 => "one",
    2 => "two",
    3 => "three",
    default => "other",
};
echo $result; // two
```

If no arm matches and there is no `default`, elephc aborts with a fatal runtime error.

## try / catch / finally / throw

```php
<?php
class DivisionByZeroException extends Exception {}

function divide($left, $right) {
    if ($right == 0) {
        throw new DivisionByZeroException();
    }
    return intdiv($left, $right);
}

try {
    echo divide(10, 2) . PHP_EOL;
    echo divide(10, 0) . PHP_EOL;
} catch (DivisionByZeroException $e) {
    echo "caught" . PHP_EOL;
} finally {
    echo "cleanup" . PHP_EOL;
}
```

Supported subset:

- built-in `Exception` class and `Throwable` interface are available without declaring them
- `Exception` provides `$message`, `__construct($message = "")`, and `getMessage()`
- `throw <expr>;` where `<expr>` has an object type implementing `Throwable`
- `throw <expr>` can also be used inside expressions such as `??` and ternaries
- `catch (ClassName $e)` and `catch (TypeA | TypeB $e)` for multi-catch
- `catch (Exception)` without binding the exception variable
- catch types must extend or implement `Throwable`
- multiple `catch` clauses
- `try { ... } finally { ... }`
- `return`, `break`, and `continue` run enclosing `finally` blocks before leaving
