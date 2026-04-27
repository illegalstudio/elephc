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
| `__FUNCTION__` | string | FQN of the enclosing function, unqualified method name inside a method, PHP-style closure marker inside a closure, or empty string outside any function |
| `__CLASS__` | string | FQN of the enclosing class, the importing class for trait members, or empty string outside any class |
| `__METHOD__` | string | `"Class::method"` inside a class method, `"Trait::method"` inside a trait method, FQN function name in a free function, PHP-style closure marker inside a closure, or empty string outside any function |
| `__NAMESPACE__` | string | The current namespace, or empty string outside any namespace |
| `__TRAIT__` | string | FQN of the enclosing trait, or empty string outside any trait |

All eight names are matched case-insensitively, matching PHP (for example, `__dir__` and `__DIR__` are the same magic constant).

## When the substitution happens

Each magic constant is lowered to a plain literal **before type checking and code generation run**, so it interacts correctly with constant folding and other downstream passes.

- `__LINE__` is replaced with `IntLiteral(span.line)` at parse time.
- `__FILE__` and `__DIR__` are replaced after parsing each source file (including any `include`d / `require`d files), against that file's canonical path.
- `__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, and `__TRAIT__` are replaced per source file before that file is inlined, so included files keep their own namespace and lexical scope.
- `__CLASS__` occurrences imported from a trait are rebound when the trait is flattened into the concrete class. `__METHOD__` and `__TRAIT__` keep the trait declaration identity, as they do in PHP.

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
        echo __CLASS__;    // class that uses the trait, e.g. "App\Util\Task"
        echo __METHOD__;   // "App\Util\Reportable::report"
        echo __TRAIT__;    // "App\Util\Reportable"
    }
}
```

### Closures

```php
<?php
$f = function() {
    echo __FUNCTION__;     // "{closure:/abs/path/main.php:2}"
};
$f();
```

Inside a method, closures use the method as their context:

```php
<?php
class Greeter {
    public function hello() {
        $f = function() {
            echo __METHOD__; // "{closure:Greeter::hello():4}"
        };
        $f();
    }
}
```

## Includes and `__FILE__` / `__DIR__`

When a file is `include`d or `require`d, the magic constants inside that file expand against **its own** file path, namespace, and lexical scope — not the file or function that included it. This matches PHP's behavior.

```php
// /app/main.php
<?php
require __DIR__ . '/lib/util.php';

// /app/lib/util.php
<?php
namespace App\Lib;

echo __FILE__;             // "/app/lib/util.php" (not /app/main.php)
echo __DIR__;              // "/app/lib"          (not /app)
echo __NAMESPACE__;         // "App\Lib"
```

`__DIR__`, `__FILE__`, and the other string-valued magic constants are also accepted in `include` / `require` path expressions (along with concatenation, string literals, and `const` / `define()`-d string constants). See [Namespaces & Includes](namespaces.md) for the full list of accepted path expressions.
