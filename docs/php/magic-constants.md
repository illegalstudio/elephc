---
title: "Magic Constants"
description: "PHP magic constants (__DIR__, __FILE__, __LINE__, __FUNCTION__, __CLASS__, __METHOD__, __NAMESPACE__, __TRAIT__) resolved at compile time to plain string or integer literals."
sidebar:
  order: 11
---

PHP defines a set of *magic constants* that change value depending on where they appear in the source. elephc supports all eight, lowering each to a plain string or integer literal at compile time. They behave identically to PHP for every common case.

| Constant | Type | What it expands to |
|---|---|---|
| `__DIR__` | string | Canonical absolute path of the directory containing the source file |
| `__FILE__` | string | Canonical absolute path of the source file |
| `__LINE__` | int | Line number of the constant in the source file |
| `__FUNCTION__` | string | FQN of the enclosing function, or empty string outside any function |
| `__CLASS__` | string | FQN of the enclosing class, or empty string outside any class |
| `__METHOD__` | string | `"Class::method"` (FQN class) inside a method, FQN function name in a free function, or empty string outside any function |
| `__NAMESPACE__` | string | The current namespace, or empty string outside any namespace |
| `__TRAIT__` | string | FQN of the enclosing trait, or empty string outside any trait |

All eight names are matched **case-sensitively** in uppercase form (e.g., `__dir__` is a plain identifier, not the magic constant).

## When the substitution happens

Each magic constant is lowered to a plain literal **before type checking and code generation run**, so it interacts correctly with constant folding and other downstream passes.

- `__LINE__` is replaced with `IntLiteral(span.line)` at parse time.
- `__FILE__` and `__DIR__` are replaced after parsing each source file (including any `include`d / `require`d files), against that file's canonical path.
- `__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, and `__TRAIT__` are replaced after the file is fully assembled (with includes inlined), based on the lexical position of each occurrence.

Because the substitution happens before the optimizer's constant-folding pass, expressions like `__DIR__ . '/file.php'` collapse into a single string literal at compile time:

```php
echo __DIR__ . '/lib/util.php';
// becomes (at compile time):
//   echo '/path/to/dir/lib/util.php';
```

## Examples

### File and directory

```php
<?php
echo __FILE__, "\n";  // "/abs/path/main.php"
echo __DIR__, "\n";   // "/abs/path"
echo __LINE__, "\n";  // "3"
```

### Function and method

```php
<?php
namespace App;

function greet() {
    echo __FUNCTION__;     // "App\greet"
    echo __METHOD__;       // "App\greet"
}

class Greeter {
    public function hello() {
        echo __CLASS__;    // "App\Greeter"
        echo __METHOD__;   // "App\Greeter::hello"
        echo __FUNCTION__; // "hello"
    }
}
```

### Namespace and trait

```php
<?php
namespace App\Util;

echo __NAMESPACE__;        // "App\Util"

trait Reportable {
    public function report() {
        echo __TRAIT__;    // "App\Util\Reportable"
    }
}
```

### Closures

```php
<?php
$f = function() {
    echo __FUNCTION__;     // "{closure}"
};
$f();
```

## Includes and `__FILE__` / `__DIR__`

When a file is `include`d or `require`d, the magic constants inside that file expand to **its own** path — not the path of the file that included it. This matches PHP's behavior.

```php
// /app/main.php
<?php
require __DIR__ . '/lib/util.php';

// /app/lib/util.php
<?php
echo __FILE__;             // "/app/lib/util.php" (not /app/main.php)
echo __DIR__;              // "/app/lib"          (not /app)
```

`__DIR__` and `__FILE__` are also accepted in `include` / `require` path expressions (along with concatenation, string literals, and `const` / `define()`-d string constants). See [Namespaces & Includes](namespaces.md) for the full list of accepted path expressions.

## Known limitations

- **Closures inside class methods**: PHP 8.4 introduced a verbose closure-name format (e.g. `{closure:App\C::m():12}`) for `__FUNCTION__` and `__METHOD__` inside closures. elephc returns the plain `{closure}` marker (matching PHP 8.0–8.3 behavior). The class / namespace / trait constants are unaffected and resolve to the lexical enclosing scope as expected.
- **`__CLASS__` / `__METHOD__` inside a trait method**: in PHP these evaluate at runtime to the *using* class. elephc resolves them at compile time at the trait declaration site, so they expand to `""` and `TraitName::method` respectively. `__TRAIT__` itself is correct in both cases. If you need runtime-bound class identity inside a trait method, use `static::class` or `self::class` instead.
