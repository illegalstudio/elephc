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
- Included files keep their own namespace and imports; an include cannot inherit the caller's namespace scope

## Case sensitivity

elephc follows PHP's symbol case rules:

- PHP keywords are case-insensitive (`IF`, `Echo`, and `function` are equivalent)
- Built-in and user-defined function calls are case-insensitive, including string-literal callback names used by `function_exists()`, `call_user_func()`, `array_map()`, and related callback built-ins
- Class, interface, trait, and method lookup is case-insensitive
- Variables, object properties, string array keys, and user-defined constants remain case-sensitive
- Built-in constant names such as `PHP_OS`, `INF`, and `STDOUT` remain case-sensitive

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

Paths are resolved at compile time and inlined. Paths are relative to the
including file.

| Form | Missing file | Already included |
|---|---|---|
| `include` | Skipped | Re-included |
| `require` | Compile error | Re-included |
| `include_once` | Skipped | Skipped |
| `require_once` | Compile error | Skipped |

Both `include 'f';` and `include('f');` syntax supported.

`include_once` and `require_once` use a runtime guard per resolved file. The
guard is shared across top-level code, functions, closures, methods, loops, and
branches, so a file is marked as included only when execution reaches the
include point. Skipped branches do not make a later `include_once` skip the
file, and repeated calls or loop iterations do not re-run a `*_once` file.

Function, class, interface, trait, enum, packed-class, and extern declarations
from statically-resolved include targets are discovered before name resolution
and type checking. This lets declarations included through loader functions,
branches, or nested include files participate in normal symbol resolution,
while executable top-level statements from included files still run at their
include point.

Declaration discovery is path-aware for the same resolved regular include
target across mutually exclusive `if` / `elseif` / `else` branches, so the same
file is not treated as redeclared just because it appears in multiple exclusive
branches. Sequential regular includes and regular includes that can repeat
through loops still report duplicate declaration errors, matching PHP's
redeclaration behavior.

When mutually exclusive branches in the same direct `if` / `elseif` / `else`
chain include different files that declare the same function name, elephc
accepts the pattern only if the function signatures match exactly. Each branch
declaration is compiled as a hidden implementation, and the public function name
dispatches to the implementation loaded by the branch that actually ran. A
function declared only by an optional conditional include uses the same runtime
marker, so the function is callable only after that include point has executed;
`function_exists()` reads the same marker for these conditional functions.
Class-like declarations remain strict: duplicate class, interface, trait, enum,
packed-class, or extern names still report redeclaration errors unless they are
separated by namespace.

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
- String-valued magic constants (`__DIR__`, `__FILE__`, `__FUNCTION__`, etc.)
- References to `const` / `define()`-d string constants — the constant must be defined **before** the include statement (ordering matches PHP runtime semantics)
- Namespace-aware constant references, including `use const` aliases

Runtime-dynamic path expressions are rejected during include resolution. The
AOT compiler only has the source files available at compile time, so it cannot
ask the generated binary to discover and inline new PHP files at runtime.

Rejected (compile error):

- Variables (`$path`)
- Function calls (`getenv('PATH')`)
- Non-constant expressions (ternaries, dynamic property access, etc.)

`const` declarations used in path expressions follow the same namespace rules as PHP: unqualified names first check `use const`, then the current namespace, then the global namespace. `define()` creates a global constant unless the string name contains a namespace separator.

`const` or `define()` calls inside functions, methods, loops, and branches are scoped to that resolved body during include expansion. They do not leak into the surrounding top-level include path resolver.

**Other limitations:** Included files must start with `<?php`. Runtime-dynamic include paths are not supported by the current AOT resolver.

## Constants
```php
<?php
const MAX_RETRIES = 3;
define("PI", 3.14159);
```
`const` declarations are namespace-aware and resolved at compile time. `define()` string names are global unless they contain an explicit namespace separator. Values must be literals or compile-time-foldable string concatenations when used by include path resolution.

## Predefined constants

| Constant | Type | Value |
|---|---|---|
| `PHP_EOL` | string | `"\n"` |
| `PHP_OS` | string | `"Darwin"` on macOS targets, `"Linux"` on Linux targets |
| `DIRECTORY_SEPARATOR` | string | `"/"` |
| `STDIN` | resource | Standard input stream |
| `STDOUT` | resource | Standard output stream |
| `STDERR` | resource | Standard error stream |
| `PATHINFO_DIRNAME` | int | 1 |
| `PATHINFO_BASENAME` | int | 2 |
| `PATHINFO_EXTENSION` | int | 4 |
| `PATHINFO_FILENAME` | int | 8 |
| `PATHINFO_ALL` | int | 15 |
| `FNM_NOESCAPE` | int | Target-specific libc/PHP value |
| `FNM_PATHNAME` | int | Target-specific libc/PHP value |
| `FNM_PERIOD` | int | 4 |
| `FNM_CASEFOLD` | int | 16 |

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
