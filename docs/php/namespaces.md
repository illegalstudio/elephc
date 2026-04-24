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

**Limitations:** Path must be a string literal. Included files must start with `<?php`.

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
| `PHP_OS` | string | `"Darwin"` |
| `DIRECTORY_SEPARATOR` | string | `"/"` |
| `STDIN` | int | 0 |
| `STDOUT` | int | 1 |
| `STDERR` | int | 2 |

`PHP_OS` currently returns `"Darwin"` on all targets, including Linux targets.

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
