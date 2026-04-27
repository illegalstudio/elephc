---
title: "Namespaces"
description: "Namespace declarations, use imports, name resolution, include/require."
sidebar:
  order: 9
---

## Declaring a namespace
```php
<?php
namespace App\Core;
function version() { return "1.0"; }
```

Block form:
```php
<?php
namespace App\Core {
    class Clock {
        public static function now() { return "tick"; }
    }
}
```

## Importing with use
```php
<?php
use App\Support\Response;
use function App\Support\render as render_page;
use const App\Support\STATUS_OK;
```

Supported forms: `use Foo\Bar;`, `use Foo\Bar as Baz;`, `use function`, `use const`, group use `use Vendor\Pkg\{Thing, Other as Alias};`, mixed group use.

## Name resolution rules
- Unqualified class names honor `use` aliases, otherwise resolve relative to current namespace
- Functions/constants: `use function`/`use const` aliases first, then current namespace, then global fallback
- Fully-qualified `\Lib\Tool` always refers to global canonical name
- Included files keep their own namespace

## Namespaces and callbacks
String-literal callback names follow the same resolution rules as function calls.

## Include / Require
```php
<?php
include 'helpers.php';
require 'config.php';
include_once 'utils.php';
require_once 'lib.php';
```

All resolved at compile time (inlined). Paths relative to including file.

| Form | Missing file | Already included |
|---|---|---|
| `include` | Skipped | Re-included |
| `require` | Compile error | Re-included |
| `include_once` | Skipped | Skipped |
| `require_once` | Compile error | Skipped |

Both `include 'f';` and `include('f');` syntax supported.

### Path expressions

The path may be any **compile-time-constant string expression**:

```php
<?php
require __DIR__ . '/lib/util.php';      // magic constant + concat
const BASE = __DIR__ . '/lib';
require BASE . '/util.php';             // const reference
define('PLUGIN', 'plugins/auth');
require_once PLUGIN . '/init.php';      // define() reference
require __DIR__ . '/' . 'sub' . '/' . 'x.php';  // nested concat
```

Accepted forms (foldable at compile time):

- String literals (`'lib/x.php'`)
- Concatenations (`.`) of foldable subexpressions
- Magic constants (`__DIR__`, `__FILE__`, `__LINE__`, etc.)
- References to `const` / `define()`-d string constants — the constant must be defined **before** the include statement (ordering matches PHP runtime semantics)

Rejected (compile error):

- Variables (`$path`)
- Function calls (`getenv('PATH')`)
- Non-constant expressions (ternaries, dynamic property access, etc.)

**Other limitations:** Included files must start with `<?php`.

## Constants
```php
<?php
const MAX_RETRIES = 3;
define("PI", 3.14159);
```
Constants are global, resolved at compile time. Values must be literals.

## Predefined constants

| Constant | Type | Value |
|---|---|---|
| `PHP_EOL` | string | `"\n"` |
| `PHP_OS` | string | `"Darwin"` on macOS targets, `"Linux"` on Linux targets |
| `DIRECTORY_SEPARATOR` | string | `"/"` |
| `STDIN` | int | 0 |
| `STDOUT` | int | 1 |
| `STDERR` | int | 2 |

## Superglobals

| Variable | Type | Description |
|---|---|---|
| `$argc` | `int` | Number of CLI arguments |
| `$argv` | `array(string)` | CLI argument values |

## Comments
```php
<?php
// Single-line comment
/* Multi-line comment */
```
