---
title: "Namespaces"
description: "Namespace declarations, use imports, name resolution, include/require."
sidebar:
  order: 11
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
- Built-in and user-defined function calls are case-insensitive, including string-literal callback names used by `call_user_func()`, `array_map()`, and related callback built-ins
- Class, interface, trait, and method lookup is case-insensitive
- Variables, object properties, string array keys, and user-defined constants remain case-sensitive
- Built-in constant names such as `PHP_OS`, `INF`, and `STDOUT` remain case-sensitive

## Namespaces and callbacks
String-literal callback names follow the same resolution rules as function calls.

`function_exists()` is introspection rather than invocation: its string argument
is checked as a literal global or fully-qualified function name. It does not
apply the current namespace or `use function` imports to an unqualified string.

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

Functions discovered through includes are compiled as hidden implementations
behind public dispatchers. The dispatcher becomes callable only after a runtime
include point has activated one of those implementations, and
`function_exists()` reads the same marker. When mutually exclusive branches in
the same direct `if` / `elseif` / `else` chain include different files that
declare the same function name, elephc accepts the pattern only if the function
signatures match exactly, then dispatches to the implementation loaded by the
branch that actually ran.

Nested/composed branch combinations and `switch` cases are not treated as
mutually exclusive for duplicate function declarations yet.

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

## Composer PSR-4 autoload (static)

The compiler reads `composer.json` from the directory containing the entry `.php` file and from each `vendor/<vendor>/<package>/composer.json`. PSR-4 mappings in those files are walked at compile time, and any class your program references is resolved through the resulting index — equivalent in spirit to `composer dump-autoload --classmap-authoritative`, but executed during compilation.

```json
// composer.json
{
    "autoload": {
        "psr-4": {
            "App\\": "src/",
            "App\\Tests\\": "tests/"
        }
    }
}
```

```
src/Service/Greeter.php   // namespace App\Service; class Greeter
src/Models/User.php       // namespace App\Models; class User
main.php                  // new App\Service\Greeter() works without explicit require
```

The autoload pass runs after name resolution and iterates until every referenced class is either declared in the program or located through the index. Files that the index doesn't reach are not parsed — only the transitive class graph from your entry-point ends up in the binary.

### Other autoload sections

The compiler reads four `autoload` (and `autoload-dev`) subsections:

| Section | Behaviour |
|---|---|
| `psr-4` | Standard PSR-4 mapping. Multiple namespace prefixes resolve longest-first, matching composer's rule. Empty prefix `""` (root namespace) is supported |
| `psr-0` | Legacy PSR-0 mapping. Both namespaced prefixes (`Vendor\\Pkg\\`) and underscore-class prefixes (`Twig_`) are supported |
| `classmap` | List of files or directories to scan. Every `.php` file is parsed and its class/interface/trait/enum declarations are added to the FQN→file index. Useful for non-PSR code |
| `files` | List of files that must always be inlined at compile time, regardless of which classes the program references. Spliced into the program at the start of the autoload pass |
| `exclude-from-classmap` | Glob patterns that drop matching files from `classmap` scanning. Supports `*` (within a path segment), `**` (across segments), `?` (single character). A trailing `/` is the directory shorthand and is rewritten as `<pattern>**` |

`autoload-dev` is always merged in alongside `autoload`. There is no production/test split in the AOT model — both contribute to the same compiled binary.

### `class_exists` / `interface_exists` / `trait_exists` / `enum_exists`

When called with a string literal class name and the default `$autoload = true`, the compiler treats the literal as an explicit demand to load the class:

```php
if (class_exists("App\\Probe", true)) {     // App\Probe is autoloaded at compile time
    $p = new App\Probe();
}
```

If `$autoload` is `false`, no compile-time load is forced; the call returns whether the class is otherwise compiled in. The class/interface/trait/enum name and the optional `$autoload` flag must be literals in the current AOT model. `function_exists` is intentionally not in this list: PHP doesn't autoload functions, and Composer's `autoload.files` is the right tool for forced function inclusion.

### Introspection helpers

| Function | Behaviour |
|---|---|
| `get_declared_classes()` | Returns the indexed array of every compiled class name. Internal/builtin names are emitted first in deterministic order; user declarations follow source order |
| `get_declared_interfaces()` | Same as above for interfaces |
| `get_declared_traits()` | Returns user-declared trait names in source order |
| `spl_classes()` | Returns the indexed array of SPL/core class and interface names shipped by the compiler today (currently 51 entries: SPL/core interfaces, throwable types, SPL exception classes, runtime-backed Phase 4 containers, and Phase 5 storage/decorator iterators). The list grows as later phases ship more SPL types |
| `spl_object_id($obj)` | The object's heap pointer cast to int — unique per object, stable per process |
| `spl_object_hash($obj)` | The object's heap pointer formatted as a decimal string. PHP returns a 32-character hex string; we return a decimal string. Both are unique-per-object and stable per-process — only the textual format differs |
| `get_class($obj)` | Resolves to the argument's static type name. Inside a method called with no argument, returns the current class context |
| `get_parent_class($obj)` | Returns the parent class name from `ctx.classes[name].parent`, or empty string when the class has no parent |
| `class_implements($object_or_class, bool $autoload = true)` | Returns an associative `interface => interface` array for a class/object, including inherited parent interfaces. When the argument names an interface, returns that interface's parent interfaces |
| `class_parents($object_or_class, bool $autoload = true)` | Returns an associative `parent => parent` array, starting with the immediate parent and then ancestors |
| `class_uses($object_or_class, bool $autoload = true)` | Returns an associative `trait => trait` array for traits used directly by the class or trait declaration. Parent class traits and traits imported by those traits are not included, matching PHP |
| `is_a($obj, "Foo")` | Compile-time fold when the second argument is a string literal: returns `true` when the object's static type equals `Foo`, descends from it, or implements it as an interface |
| `is_subclass_of($obj, "Foo")` | Same as `is_a` but excludes the case where the static type *is* `Foo` |
| `class_alias($original, $alias)` | At compile time, top-level literal calls synthesize `class $alias extends $original {}`. The alias is realised as a *subclass* rather than a true name alias: `new $alias()`, `$obj instanceof $alias`, and `$alias::CONST` work; `(new $original()) instanceof $alias` returns `false` (it would be `true` under PHP runtime semantics). Runtime-dynamic call shapes are rejected because elephc cannot mutate the class table after compilation |

`class_implements`, `class_parents`, and `class_uses` accept either an object
expression with a known static class type or a string literal class-like name.
The optional `$autoload` flag must be a literal bool or int in the current AOT
model. A string name that is not compiled into the binary returns `false`,
matching PHP's unresolved-name result.

### Closure-based autoload (`spl_autoload_register`)

The compiler accepts three call shapes for `spl_autoload_register`:

```php
// 1. Closure literal
spl_autoload_register(function ($name) {
    require_once __DIR__ . '/lib/' . $name . '.php';
});

// 2. Closure stored in a top-level variable
$loader = function ($name) {
    require_once __DIR__ . '/lib/' . $name . '.php';
};
spl_autoload_register($loader);

// 3. First-class callable referenced by function-name string
function myAutoloader($name) {
    require_once __DIR__ . '/lib/' . $name . '.php';
}
spl_autoload_register('myAutoloader');
```

In all three cases, the body is evaluated symbolically against each candidate class name — a typical PSR-0 / custom autoloader works as-is:

```php
<?php
spl_autoload_register(function ($name) {
    require_once __DIR__ . '/lib/' . str_replace('\\', '_', $name) . '.php';
});

$u = new App_User("Ada");        // loads lib/App_User.php at compile time
```

When `autoload::run` encounters an unknown class, it tries the composer.json PSR-4 index first and then runs each registered closure with `$name` bound to the candidate FQN. The first closure that produces an existing file path wins. The rest of the program never sees the closure body — the register site is consumed at compile time and stripped.

The interpreter understands a deliberate subset of PHP, enough for typical autoloaders:

- string literals, `.` concatenation, magic constants (`__DIR__`, `__FILE__` — already substituted before the resolver)
- variable reads/writes (`$path = ...; require_once $path;`)
- `if`/`elseif`/`else` whose conditions fold to a literal bool
- `str_replace`, `str_starts_with`, `str_ends_with`, `strtolower`, `strtoupper` with literal arguments
- `sprintf` with `%s` placeholders (and the `%%` literal escape)
- `dirname` (with optional levels), `basename`, `realpath` (returns `false` when the path doesn't resolve, matching PHP), and `pathinfo($path, PATHINFO_*)` for `DIRNAME` / `BASENAME` / `EXTENSION` / `FILENAME`
- `file_exists`, `is_file`, `is_readable`, `is_dir` against the actual filesystem at compile time
- `require`, `require_once`, `include`, `include_once` (the call that produces the autoload path)
- `return` (stops without including)

`spl_autoload_register` callsites can also live inside a top-level `if`, `if/else`, or `if/elseif/else` whose condition folds to a literal bool (`if (true)`, `if (1)`, `if (PHP_OS === 'Linux')` if `PHP_OS` were a foldable constant, …). The chosen branch is inlined at compile time before collection runs. Conditions whose value the compiler can't decide leave the `if` unchanged and the inner register call is ignored.

Anything else — loops, exceptions, `new`, method calls, ternaries, match, captures via `use(...)` — silently rejects the rule for that candidate; the chain falls through to the next rule (or PSR-4).

`spl_autoload_unregister($closure)` removes a previously-registered rule when the closure AST matches an entry. `spl_autoload_call("App\\Foo")` with a literal class-name argument forces the autoload pass to resolve `App\Foo` even if the rest of the program doesn't reference it.

| Function | Behavior |
|---|---|
| `spl_autoload_register($cb, $throw = true, $prepend = false)` | Closure literal → registered as a compile-time rule (chain prepended when `$prepend = true`). Closures with captures or multiple parameters are silently rejected. Returns `true` either way |
| `spl_autoload_unregister($cb)` | Removes a previously registered rule when the closure AST matches. Returns `true` |
| `spl_autoload_functions()` | Returns an indexed array with one int placeholder per registered rule. `count()` and `foreach` reflect the rule count. The values are rule indexes (0..N-1), not actual callables |
| `spl_autoload_extensions($ext = null)` | Read or read+write a runtime-mutable string. With no arg or literal `null`, returns the current value (default `".inc,.php"`). With a string literal arg, writes the new value and returns the previous one |
| `spl_autoload_call($name)` | Literal class name → forces compile-time autoload for that class. Variable argument → no-op |
| `spl_autoload($name, $ext = null)` | Same as `spl_autoload_call` |

**Limitations:**

- `vendor/` is scanned one level deep (`vendor/<vendor>/<package>/composer.json`); nested or non-standard vendor layouts are not.
- Conditional class declarations (e.g. `if (PHP_VERSION_ID >= 70000) { class X { ... } }`) are not autoload-aware: the file is included unconditionally, and the conditional inside still applies as it would for any include.
- When the compiler consumes a closure assignment (`$cb = function ...`) or an autoloader function declaration (`function myAutoloader ...`), the source statement is **stripped from the program**. If the closure variable or function is referenced elsewhere, you'll get a clear "undefined variable / function" error at compile time. Use a separate variable / function if you need to reuse the same body outside autoloading.
- `spl_autoload_register` calls inside function bodies, methods, loops, or non-top-level positions other than foldable `if`/`else` are ignored. Only top-level (or top-level-via-foldable-if) callsites contribute rules.
- `spl_autoload_functions()` returns the same value regardless of where in the program it's called: in the AOT model the rule chain is finalized at compile time, so there is no temporal "before/after register" distinction.
- `spl_autoload_extensions()` setter calls must use a string literal. Dynamic string setters are rejected until the runtime stores the value with refcount ownership.
- Real runtime autoload (loading new code after the binary starts) is not possible in an AOT compiler.

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
