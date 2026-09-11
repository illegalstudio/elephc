---
title: "Types"
description: "Data types supported by elephc: int, float, string, bool, array, null, mixed, iterable, resource, callable, object, enum, and extension types."
sidebar:
  order: 1
---

## Data Types


| Type             | Supported        | Notes                                                                                                                  |
| ---------------- | ---------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `int`            | Yes              | 64-bit signed integer. Literals: decimal `42` / `1_000_000`, hexadecimal `0xFF` / `0xFF_FF`, legacy octal `0755` / `0_755`, explicit octal `0o755` / `0O755` (PHP 8.1+), binary `0b1010` / `0B1010` (PHP 5.4+). Numeric separators `_` allowed between digits in any base (PHP 7.4+). |
| `string`         | Yes              | Pointer + length pair, double and single quoted                                                                        |
| `null`           | Yes              | Sentinel value, coerces to `0`/`""` in operations                                                                      |
| `bool`           | Yes              | `true`/`false` as distinct type. `echo false` prints nothing, `echo true` prints `1`. Coerces to 0/1 in arithmetic.    |
| `float`          | Yes              | 64-bit double-precision. Literals: `3.14`, `.5`, `1.5e3`, `1.0e-5`, `1_000.5`, `1e1_0`. Constants: `INF`, `NAN`.       |
| `array`          | Yes              | Indexed (`[1, 2, 3]`) and associative (`["key" => "value"]`). Heterogeneous indexed and associative payloads widen to boxed `mixed`. Arrays use copy-on-write semantics. |
| `mixed`          | Yes              | Supported in type hints and typed locals. Runtime values are boxed with a per-value tag, including heterogeneous array payloads and union storage. |
| `iterable`       | Yes              | PHP pseudo-type for `array \| Traversable`. Supports indexed arrays, associative arrays, `Iterator`, and `IteratorAggregate`; runtime operations (`foreach`, `echo`, `gettype()`, `var_dump()`, `===`, casts, `is_iterable()`) dispatch on heap-kind, value-type, or interface metadata as needed. |
| `resource`       | Inferred only    | File handles and standard streams are modeled separately from integers. `fopen()` returns `resource\|false`, and `STDIN`, `STDOUT`, and `STDERR` are stream resources. PHP does not allow `resource` as a type declaration, so elephc does not accept `resource` annotations. |
| `callable`       | Yes              | Closures, arrow functions, first-class callables, and FFI callback parameters.                                         |
| `object`         | Yes              | Class instances. Heap-allocated, fixed-layout. `new ClassName(...)`; the generic `object` type hint accepts any class, interface, or enum object. |
| `enum`           | Yes              | Pure and backed enums. Cases are singletons. Backed enums support `->value`, `::from()`, `::tryFrom()`, `::cases()`.   |
| `int|string`     | Yes              | Union type — variable accepts any of the listed types. Lowered to Mixed at runtime.                                    |
| `?int`           | Yes              | Nullable shorthand — sugar for `int|null`. The explicit `T|null` form (e.g. `A|null`) is also accepted.                |
| `string|null`    | Yes              | Union with the `null` literal type. Folds to the nullable shorthand `?string`, so `string|null` and `?string` are identical. |
| `int|false`      | Yes              | Union with the `false` literal type (PHP's `strpos`-style return). `false`/`true` widen to `bool`; the runtime value is a real boolean. |
| `void`           | Return only      | Valid as a function, method, closure, arrow, or extern return type. Internally, `null` is represented as `Void`.        |
| `never`          | Return only      | Marks a function, method, closure, or interface method that **never returns** — it must always `throw`, call `exit()`/`die()`, or loop forever. Returning is rejected at type-check time. |
| `ptr` / `ptr<T>` | elephc extension | Raw 64-bit pointer, optionally carrying a checked compile-time pointee tag. See [Pointers](../beyond-php/pointers.md). |
| `buffer<T>`      | elephc extension | Fixed-size contiguous storage for POD scalars, pointers, or packed records. See [Buffers](../beyond-php/buffers.md).   |
| `packed class`   | elephc extension | Flat POD record type with compile-time field offsets. See [Packed Classes](../beyond-php/packed-classes.md).           |

Integer-form numeric literals keep the `int` type only while they fit in PHP's signed 64-bit range. Larger decimal, hexadecimal, octal, or binary literals are promoted to `float`, matching PHP on 64-bit builds.

### Literal pseudo-types in unions

PHP lets `null`, `false`, and `true` appear as type members. elephc accepts them in parameter, return, and property positions, matched case-insensitively like the other built-in type names.

```php
<?php
function find(string $haystack, string $needle): int|false {
    return strpos($haystack, $needle); // a real int, or the literal false
}

function label(?string $name): string|null {
    return $name === '' ? null : $name;
}

class Setting {
    public string|null $value = null;
}
```

Rules:

- `T|null` is exactly equivalent to the nullable shorthand `?T` — both compile to the same type, so `string|null` and `?string` are interchangeable.
- `false` and `true` widen to `bool`. elephc does not track literal-bool precision, so `int|false` is accepted wherever `int|bool` is; the stored value is still a genuine boolean at runtime.
- A multi-member union may mix these with other members (`int|string|null`); the `null` member keeps the whole union nullable.
- The nullable shorthand still may not be combined with a pipe union: write `T|null`, not `?T|U`.

### Intersection types

An intersection type `A&B` (PHP 8.1) declares a value that satisfies every listed class/interface. elephc accepts the syntax in parameter and return positions:

```php
<?php
function render(Renderable&Cacheable $widget): string {
    return $widget->render();
}
```

The `&` is recognized as an intersection only when it is followed by another type; a `&` before a variable (`int &$x`) remains the by-reference marker. The nullable shorthand cannot be combined with an intersection: `?A&B` is rejected, matching PHP.

Current limitation: the value is typed as its **first** listed member, so member access resolves against that member (`$widget->render()` above, from `Renderable`). Methods declared only on later members are not yet resolved, and argument compatibility is checked against the first member. Full structural intersection resolution is planned.

The internal `PhpType` model also includes `TaggedScalar`, which is not PHP
syntax and cannot be written in source code. Codegen uses it only for the
default tagged null representation of `int|null` values, storing an inline
`{payload, tag}` pair instead of a heap-boxed `mixed` cell.

### Never

`never` marks a function, method, closure, or interface method that **must not return normally**. The function body is expected to either `throw`, call `exit()`/`die()`, or loop forever.

```php
<?php
function panic(string $msg): never {
    throw new RuntimeException($msg);
}

class Failer {
    public function fail(): never {
        throw new \Exception("boom");
    }

    public static function bail(int $code): never {
        exit($code);
    }
}

interface Aborts {
    public function abort(): never;
}
```

Rules:

- valid as a return type for functions, closures, instance methods, static methods, and interface methods
- matched case-insensitively like PHP's built-in type names (`never`, `Never`, and `NEVER` are equivalent)
- must be used as a standalone return type; `?never`, `never|null`, and `int|never` are rejected
- not valid as a parameter, property, or typed local
- declaring `: never` and then writing `return $value;` (or even bare `return;`) is rejected at type-check time
- `: never` is the **bottom type** in the type system: it is a subtype of every other type, so a child method may override a parent that returns `void`/`int`/etc. with `: never`
- the reverse is rejected: a parent or interface method declared as `: never` requires the child/implementation to declare a compatible return type
- if execution falls through a `: never` function or method body, elephc emits a runtime fatal error instead of returning to the caller

### Typed local declarations

```php
<?php
int|string $value = 1;
?int $maybe = null;
```

Rules:

- union types are supported in typed local declarations, for example `int|string`
- nullable shorthand `?T` is supported as sugar for `T|null`
- at runtime these values are lowered to the compiler's boxed tagged representation
- `?T|U` is not accepted; write `T|U|null` explicitly instead
- method calls and property access work on object unions — a single object class plus scalars (`A|false`, `A|null`) and unions of two or more distinct object classes (`A|B`, `A|B|false`). The method/property must exist on **every** object member; codegen dispatches on the runtime class id, and a non-object runtime value faults like PHP

### Property type declarations

```php
<?php
class User {
    public int $id;
    public string $name = "Ada";
    public ?string $email = null;
    public static int $count = 0;
}
```

Rules:

- instance and static properties can use declared property types
- property defaults and assignments must be compatible with the declared type
- constructor assignments through untyped parameters are checked once call sites refine the parameter type
- nullable and union property storage is boxed using the same mixed runtime shape as typed locals; scalar literal defaults (`int|string $v = 1`, `float|int $v = 1.5`, `bool|int $v = true`) are boxed into that shape
- static property redeclarations across inheritance follow PHP-style rules: non-private inherited properties keep invariant declared types, cannot reduce visibility, and cannot override `final` properties
- private inherited static properties can be redeclared as independent subclass slots
- untyped inherited static properties cannot be redeclared with a type, and typed inherited static properties cannot be redeclared without one
- direct element writes to static array properties, such as `ClassName::$items[] = $value` or `ClassName::$items[0] = $value`, require the property to be an `array`
- `void` and `callable` are not valid property types

### Null behavior

```php
<?php
$x = null;
echo $x;              // prints nothing
echo is_null($x);     // prints 1
echo $x + 5;          // prints 5 (null → 0)
echo $x . "hello";    // prints "hello" (null → "")
$x = 42;              // reassignment from null works
```

### Type Casting

```php
$i = (int)3.7;       // 3
$f = (float)42;      // 42.0
$s = (string)42;     // "42"
$b = (bool)0;        // false
$a = (array)42;      // [42]
$o = (array)$obj;    // property name => value hash, with PHP's visibility-mangled keys
$m = (array)$mixed;  // dispatches on the runtime tag: arrays pass through, scalars wrap, objects project
```

`(array)` on an object projects all of its properties into a string-keyed hash
using PHP's exact key mangling — `x` for a public property, `"\0*\0y"` for a
protected one, `"\0Class\0z"` for a private one — including the
`__PHP_Incomplete_Class` payload a restricted `unserialize()` produces. For the
scope-aware, unmangled view use `get_object_vars()`. `(array)` on a boxed
`mixed` value dispatches on the runtime tag at run time.

Cast names and aliases are case-insensitive, matching PHP. For example,
`(INT)`, `(Integer)`, and `(integer)` are equivalent.

`(int)` and `intval()` on a `float` follow PHP's `zend_dval_to_lval` rules on every supported
target: `NAN` and `±INF` become `0`, an in-range value truncates toward zero, and any other
out-of-range value is reduced modulo 2^64 before being read back as a signed 64-bit integer
(so `(int)1e300` is `0` and `(int)1.5e19` is `-3446744073709551616`). The same conversion is
used for float array keys, so `$a[NAN]` and `$a[INF]` both write index `0`.

Aliases: `(integer)`, `(double)`, `(real)`, `(boolean)`.

### Type functions


| Function        | Signature                    | Description                    |
| --------------- | ---------------------------- | ------------------------------ |
| `is_null()`     | `is_null($val): bool`        | Returns true if null           |
| `is_float()`    | `is_float($val): bool`       | Returns true if float          |
| `is_int()`      | `is_int($val): bool`         | Returns true if integer        |
| `is_string()`   | `is_string($val): bool`      | Returns true if string         |
| `is_numeric()`  | `is_numeric($val): bool`     | Returns true if int or float   |
| `is_bool()`     | `is_bool($val): bool`        | Returns true if bool           |
| `is_array()`    | `is_array($val): bool`       | Returns true if indexed or associative array |
| `is_object()`   | `is_object($val): bool`      | Returns true if value is an object |
| `is_scalar()`   | `is_scalar($val): bool`      | Returns true for int, float, string, or bool (not null, array, object, or resource) |
| `is_iterable()` | `is_iterable($val): bool`    | Returns true if array or Traversable-compatible iterable |
| `is_callable()` | `is_callable($val): bool`    | Returns true for closures, first-class callables, strings case-insensitively naming known builtins, user functions, or public static methods (`"Class::method"`), `[$obj, "method"]` arrays with public methods, `[ClassName::class, "method"]` static method arrays, and objects with public `__invoke()`. |
| `is_resource()` | `is_resource($val): bool`    | Returns true if value is an open resource handle |
| `is_nan()`      | `is_nan($val): bool`         | Returns true if NAN            |
| `is_finite()`   | `is_finite($val): bool`      | Returns true if not INF/NAN    |
| `is_infinite()` | `is_infinite($val): bool`    | Returns true if INF or -INF    |
| `boolval()`     | `boolval($val): bool`        | Convert to bool                |
| `floatval()`    | `floatval($val): float`      | Convert to float               |
| `intval()`      | `intval($value, $base = 10): int` | Converts to integer. `$base` applies only to a string `$value`; base `0` auto-detects `0x`/`0b`/a leading octal zero |
| `strval()`      | `strval($val): string`       | Convert to string              |
| `gettype()`     | `gettype($val): string`      | Returns type name              |
| `empty()`       | `empty($val): bool`          | Returns true if value is falsy |
| `unset()`       | `unset($var, ...$vars): void` | Unbinds one or more variables. On an eligible local a later read is a compile error, in both modes — see [Local retyping](#local-retyping) |
| `settype()`     | `settype($var, $type): bool` | Changes variable type in place |

PHP's predicate aliases are supported and behave identically to their canonical
forms: `is_integer()` and `is_long()` are aliases of `is_int()`, and
`is_double()` and `is_real()` are aliases of `is_float()`.

### Type narrowing

Inside an `if` (or `if`/`elseif`*/`else` chain) guarded by a type predicate on a variable, that variable is narrowed to the tested type within the matching branch(es), so it can be used as that type without an explicit cast. `is_int()`, `is_float()`, `is_string()`, and `is_bool()` (and their aliases) narrow to the matching scalar, and `$x instanceof SomeClass` narrows to that class — including calling its methods. Each subsequent `elseif`, and the `else` branch, see the complement of all previous guards. The statements *after* the whole construct also see the complement when the chain is exhaustive by divergence — there is no `else` and *every* clause body always diverges (`return`, `throw`, `exit()`, `die()`, or a call to a `: never` function) — because reaching them means every guard was false. A leading `!` flips the then/else branches.

```php
function describe($x): string {        // $x may be int or a Point across call sites
    if (is_int($x)) {
        return "int " . ($x + 1);      // $x is int here
    }
    return "point " . $x->label();     // $x is the object here
}
```

Narrowing is not tracked across a reassignment of the variable inside the branch.

Narrowing applies to function and method parameters. A parameter whose call sites pass incompatible types (e.g. `int` at one site and a class instance at another) is inferred as a union, and the guard narrows it inside each branch. This is **not** yet supported for closure parameters: a closure invoked with incompatible argument types is rejected at compile time rather than inferred as a union.

### Local retyping

An **undeclared-type** local is monomorphic by default, but three shapes let it change type anyway. `unset($a)` ends the binding, and the next assignment re-binds `$a` at any type with no diagnostic. A plain straight-line reassignment (`$a = 0; $a = "ciao";`) re-binds `$a` to a fresh slot of the new type and warns. A branch-divergent assignment (`if (…) { $a = 0; } else { $a = "ciao"; }`) compiles the local as boxed `mixed` storage for the whole body and warns — a performance signal as much as a correctness one, since every read of it then goes through the box.

One behaviour differs from PHP: reading a variable after `unset()` is a compile error, where PHP warns and evaluates the read as `null`. Probe the name with `isset()` (or `empty()` / `??`), which stay legal on an unbound name:

```php
$a = "x";
unset($a);
echo $a;                            // Undefined variable: $a — compile error
echo isset($a) ? "set" : "unset";   // fine: prints "unset"
```

None of this touches a **declared** type: a typed local, a type-hinted parameter, and a class property stay strict in every mode. `--strict-locals` turns the two warning shapes back into hard errors; see [Strict locals mode](../compiling/cli-reference.md#strict-locals-mode) for the flag and for which names are eligible.

### Parameter type coercion

PHP's default (coercive) mode converts a scalar argument to a declared scalar parameter type when the call runs. elephc applies the same conversions, but only where it can reproduce PHP's result exactly. Everything in this section describes a file **without** `declare(strict_types=1)`; see [Strict types](#strict-types) for what the directive changes.

**Always accepted.** These conversions are total (every value converts), produce no PHP diagnostic, and use the same code as the corresponding explicit cast, so they match PHP byte for byte for both literals and runtime values:

| Declared parameter | Accepts | Example |
|---|---|---|
| `string` | `int`, `float`, `bool` | `f(42)` → `"42"`, `f(4.5)` → `"4.5"`, `f(false)` → `""` |
| `bool` | `int`, `float`, `string` | `f("0")` → `false`, `f(-0.5)` → `true` |
| `float` | `int`, `bool` | `f(5)` → `5.0` |
| `int` | `bool` | `f(true)` → `1` |

**Accepted for compile-time constants.** A `float` or numeric-string *literal* binds to an `int` or `float` parameter when PHP's conversion is exact:

```php
function takesInt(int $i) { return $i; }
function takesFloat(float $f) { return $f; }

echo takesInt(5.0);      // 5
echo takesInt("42");     // 42
echo takesInt(" 42 ");   // 42   — PHP allows surrounding whitespace
echo takesFloat("1e3");  // 1000
```

**Rejected at compile time.** Every remaining `float`/`string` → `int` and `string` → `float` binding is a compile error naming the PHP behaviour, because PHP decides those cases at run time with a channel elephc does not have at a parameter boundary:

- A **lossy** conversion. PHP emits `Deprecated: Implicit conversion from float 5.5 to int loses precision` and passes `5`; elephc has no runtime deprecation channel (the same gap documented for `++`/`--` below), so it refuses rather than dropping the notice.
- A **non-numeric or partially numeric** string. PHP throws `TypeError` — note this differs from the `(int)` cast, where `(int)"42abc"` is `42`.
- A **runtime-valued** `float` or `string`. Which of the three PHP outcomes applies is only knowable at run time.

Add the explicit cast the call site means — `takesInt((int) $f)`, `takesFloat((float) $s)` — or `intval()`/`floatval()`.

**Not covered.** Coercive binding applies to by-value declared parameters of user-declared functions, methods, static methods and constructors:

- **Pass-by-reference parameters** (`function f(string &$s)`) stay strict. PHP converts the caller's variable in place and writes the converted value back; elephc would have to pass a converted temporary, silently dropping the callee's writes, so the call is rejected instead.
- **Builtin functions** keep their own per-builtin argument rules.
- **Classes injected by the compiler** (SPL, `Exception`, reflection) stay strict, because several of their members are lowered by dedicated emitters rather than the shared argument path.
- **Nullable and union parameters** (`?string $s`, `int|string $v`) stay strict; only a plain declared scalar type binds coercively.

### Strict types

A file that opens with `declare(strict_types=1);` gets PHP's strict parameter binding: a declared scalar parameter accepts only an argument of exactly that type, plus the single widening of `int` into a declared `float`. Every conversion in the tables above becomes a compile error naming the `TypeError` PHP would throw at run time.

| Argument type → | `int` | `float` | `string` | `bool` |
|---|---|---|---|---|
| **declared `int`** | accepted | rejected | rejected | rejected |
| **declared `float`** | accepted (widened) | accepted | rejected | rejected |
| **declared `string`** | rejected | rejected | accepted | rejected |
| **declared `bool`** | rejected | rejected | rejected | accepted |

```php
<?php

declare(strict_types=1);

function takesInt(int $i) { return $i; }
function takesFloat(float $f) { return $f; }

echo takesFloat(42);       // 42   — the one implicit conversion strict mode keeps
echo takesInt((int) "7");  // 7    — an explicit cast is always accepted
echo takesInt(true);       // compile error: must be of type int, bool given
echo takesInt("42");       // compile error: must be of type int, string given
```

The directive is scoped to the physical file it appears in, exactly as in PHP. It does not propagate into included files, and the file containing the **call site** decides — not the file declaring the callee:

```php
// lib.php  (no declare)
function coerceHere(int $i) { return $i; }
function fromLooseFile() { return coerceHere(true); }   // still coerces to 1

// main.php
declare(strict_types=1);
require __DIR__ . '/lib.php';
echo fromLooseFile();       // 1 — the call above lives in a coercive file
echo coerceHere(true);      // compile error — this call lives in a strict file
```

**What the directive covers.** Declared by-value parameters of user functions, methods, static methods, constructors, closures, arrow functions, first-class callables, declared variadic element types, and `call_user_func`/`call_user_func_array` — which forward the caller's frame in PHP and therefore stay strict.

**What it does not cover.**

- **Callbacks invoked by other internal functions** (`array_map`, `usort`, `array_walk`, `preg_replace_callback`, …) keep coercive binding, matching PHP: the engine calls them from its own frame, which never carries the directive. `array_map('g', [true])` still passes `1` to `g(int $i)` in a strict file.
- **Builtin function arguments** keep their own per-builtin rules; the directive does not tighten them. PHP throws `TypeError` for `strlen(42)` under `strict_types=1` while elephc still applies each builtin's own argument checking.
- **Return types, typed property assignments and typed constants** are unaffected; PHP also applies `strict_types` to those.
- **Positional spread arguments** (`f(...$args)`) and **variadic parameters of an inline closure literal** (`function (int ...$xs) {}`) are not checked against the declared parameter type, in strict or coercive mode.
- **Values whose type is only known at run time** (`mixed`, union-typed and array-element values) are not rejected: elephc cannot raise PHP's runtime `TypeError` at a parameter boundary, so such a binding is left to the existing compatibility rules.

### Callable strings

PHP accepts a function-name string wherever a `callable` is declared and resolves it when the call runs. elephc resolves callables statically, so a callable string binds when it is a compile-time constant:

```php
function apply(callable $f, string $s) { return $f($s); }

echo apply("strtoupper", "abc");        // ABC   — builtin name
echo apply("my_helper", "abc");         //       — user function name
echo apply("Formatter::wrap", "abc");   //       — "Class::method"
```

Names are matched case-insensitively and a leading `\` is allowed, matching PHP. A callable string that is only known at run time is rejected with `a callable string must be a compile-time constant here`; pass a first-class callable (`strtoupper(...)`), a closure, or a literal name instead. A constant string that names nothing is rejected too, where PHP throws `TypeError` when the call runs.

Two gaps remain: array callables (`[$obj, "method"]`, `["Class", "method"]`) are still rejected at a `callable` parameter, and because elephc maps the `Closure` type hint onto the same internal callable type, a `Closure` parameter also accepts a callable string where PHP requires an actual `Closure`.

### Known incompatibilities with PHP

- `$argv[0]` returns the compiled binary path, not the `.php` file path.
- Integer `+`, `-`, and `*` overflow promotes to `float` for both constant-folded and runtime arithmetic, matching PHP on 64-bit builds. `intval()`/`(int)` of an integer-valued string near the 64-bit boundary (e.g. `intval("9223372036854775807")`) is still lossy at the string-conversion boundary.
- The `**` operator is int-preserving like PHP (`2 ** 3` is `int(8)`), but the `pow()` **function** still always returns a `float` (`pow(2, 3)` is `float(8)` where PHP gives `int(8)`). A numeric *string* operand to `**` is always computed on the floating-point path, so an integer-form string stays a `float` (`"2" ** 3` is `float(8)`, PHP gives `int(8)`); the printed value still matches. The other arithmetic operators (`+`, `-`, `*`, `/`, `%`) coerce numeric-string operands with PHP's int-or-float rules — see the numeric-string arithmetic note below.
- Arithmetic (`+ - * / % **`) accepts numeric and leading-numeric string operands and coerces them at runtime with PHP's numeric-string grammar: an integer-form string coerces to `int` (`"123" + 3` is `int(126)`), a float-form string (containing a `.` or exponent) to `float` (`"1.5" + 3` is `float(4.5)`), an integer-form string outside the 64-bit range to `float` (`"99999999999999999999" + 1` is `float(1.0E+20)`), and a leading-numeric string uses its numeric prefix (`"  +12foo" + 3` is `int(15)`). Three gaps remain. A **fully non-numeric** string with no numeric prefix (`"hi"`) is a `TypeError` in PHP; elephc reports it as a compile error when the operand is a constant string literal (`"hi" + 1`), but a non-numeric string whose value is only known at **runtime** is coerced to `0` rather than raising PHP's runtime `TypeError` — so a program that *catches* that `TypeError` does not compile. Likewise the `Warning: A non-numeric value encountered` PHP emits for a leading-numeric operand is reported at compile time for constant literals only, not for runtime string values, and `%` on a float-form string does not emit PHP's `Deprecated: Implicit conversion from float-string ... loses precision` (`"1.9" % 2` is `int(1)` in both). Relational comparison (`< <= > >= <=>`) deliberately does **not** widen to string operands this way (see the previous bullet).
- The numeric-string coercion is scoped to the binary arithmetic operators. Unary negation still rejects a `string` operand at compile time (`-"5"` is a compile error where PHP gives `int(-5)`), and so do the bitwise operators (`"6" << 1`, `&`, `|`, `^`, `>>`), which PHP 8 coerces under the same "Saner string to number conversions" rules.
- Converting an array to a string (via `.` concatenation, `echo`, or string interpolation) yields the literal `"Array"`, matching PHP's value, but elephc does not emit PHP's `E_WARNING` "Array to string conversion".
- Scalar loose comparison (`==`, `!=`) follows PHP-style bool truthiness, null-vs-empty-string, numeric-string, non-numeric string byte-comparison, and numeric `int`-vs-`float` rules for constant-folded literals and non-folded runtime scalar operands. One known gap: when an **untyped (`mixed`) operand holds a `float`** at runtime — e.g. `switch ($x)` over an untyped `$x = 1.5`, or `$x == 1` — the value is truncated to `int` before comparing, so `1.5` wrongly compares equal to `1`. Statically-typed `float` operands compare correctly; only untyped float-bearing values are affected.
- Relational comparison (`<`, `<=`, `>`, `>=`, `<=>`) between two **runtime** string operands is rejected at compile time with "Comparison operators require numeric operands" / "Spaceship operator requires numeric operands", where PHP compares them (numerically when both look numeric, byte-wise otherwise). The constant-folded form is not affected: `"a" <=> "b"`, `"B" < "a"` and `"10" > "9"` are evaluated at compile time with PHP's exact rules and produce PHP's answer, so only comparisons whose operands are not compile-time constants hit the restriction.
- `??=` is checked against typed assignment storage for variables, object properties, static properties, and non-append array elements. For concrete local variable types, the fallback must keep the same type or be a literal `null`.
- Plain array numeric casts (`(int)$array`, `(float)$array`) follow elephc's existing array cast semantics (return the element count rather than PHP's `0`/`1`). Direct `iterable` numeric casts use PHP's empty/non-empty `0`/`1` semantics.
- `__destruct` runs when an object's refcount reaches zero (scope exit, reassignment, `unset`, program end), matching PHP's timing, but **object resurrection is not supported**: re-storing `$this` so the object would outlive the destructor does not keep it alive — the object is still freed once `__destruct` returns.
- Under the compatibility `--null-repr=sentinel` opt-out, the integer `9223372036854775806` (`PHP_INT_MAX - 1`) collides with elephc's internal null marker in unboxed scalar slots and is misread as `null` by `echo`, `var_dump()`, `is_null()`, `??`, and related null checks. The default tagged null representation does not have this collision: the full 64-bit integer range round-trips.
- `match` (and ternary) arms whose scalar types share one runtime representation merge to it instead of each arm keeping its own type: an `int` arm together with a `bool` arm collapses to one representation (`match($n) { 1 => 42, default => true }` yields `bool(true)` where PHP keeps `int(42)`). Arms with otherwise distinct runtime representations — object, array, string, int, float, `null` — each keep their own runtime type, matching PHP.
- Variable variables (`$$name`, `${$expr}`) are not supported yet. Native AOT
  locals use fixed compile-time stack slots; supporting a runtime-computed name
  will require routing the access through Magician's materialized named scope
  and synchronizing the affected native locals. Use an array keyed by the
  dynamic name in portable elephc code for now.
- Reference aliases to array elements (`$b =& $a[0]`) are limited to **indexed arrays with integer indices**; associative arrays (`$b =& $a['key']`) are rejected at compile time. Referencing an out-of-range index binds the alias to a null cell instead of creating the element (PHP autovivifies it as `null`), and the alias points into the array's storage, so it is only valid while the array is alive and not reallocated by growth.
- Reference *elements* inside array literals (`$r = [&$a, &$b];`, `['k' => &$a]`, `array(&$a)`) are rejected at compile time with `Reference elements in array literals ([&$x]) are not supported`. PHP stores such an element as a reference cell aliasing the source variable, so `$r[0] = 9` writes through to `$a`. elephc's arrays hold plain values and its only reference form points *into* array storage (the `$b =& $a[0]` case above), never out of it — an element aliasing a local would be a pointer to a stack slot the array can outlive. Assign the value and copy back, or alias an existing element with `$b =& $a[0]`.
- `goto` and its target labels are not supported and are rejected at compile time (`` `goto` is not supported ``). elephc's termination analysis, flow-sensitive narrowing, loop/branch pruning, and constant propagation all read control flow from the statement tree, which an arbitrary intra-function jump invalidates. Use `break` (including `break 2;`), `continue`, a loop flag, or an early `return`. See [Control Structures](./control-structures.md#goto).
- `++` / `--` on a `string` follows PHP exactly, including the perl-style alphanumeric carry (`"az"++` is `"ba"`, `"Zz"++` is `"AAa"`) and the numeric-string retype (`"9"++` is `int(10)`, `"3.5"++` is `float(4.5)`). Because the operator can change the value's type, a `string` local that is a `++`/`--` target is given boxed `mixed` frame storage for its whole lifetime, so its runtime type follows the value rather than the declaration. The one divergence: PHP raises `E_DEPRECATED` for `++` on a non-alphanumeric string and for `--` on a non-numeric string, and elephc has no runtime deprecation channel, so it produces the same value without the notice. `++` / `--` on an array, object, buffer, or pointer local is still rejected at compile time.
- `print_r($value, true)` captures into a fixed 64 KiB buffer: rendered output longer than 65536 bytes is truncated at the cap (PHP returns the full string). Echo mode (`print_r($value)`) is unaffected.
- `var_dump()`, `print_r()` and `var_export()` render an object's **declared** properties only. Dynamic (undeclared) properties — every property of a `stdClass` built with `$o->p = 1`, and any property added to an `#[\AllowDynamicProperties]` class — are not listed, so `print_r(new stdClass)` prints an empty body where PHP lists the assigned properties. All three renderers share one per-class descriptor, so they never disagree about which properties an object has.
- `func_num_args()`, `func_get_args()` and `func_get_arg()` are compiled away rather than dispatched as builtin calls, so `function_exists()` reports `false` for the three names where PHP reports `true`. Their supported scopes are also narrower than PHP's: they are rejected in a function with an optional (defaulted) parameter, in a function that already declares its own variadic, and in a method that overrides a parent method or implements an interface method. Everywhere else — functions, methods, static methods, closures, arrow functions, generators — they match PHP, including reporting the current values of the declared parameters. See [Functions](./functions.md#argument-introspection).
- Surplus *positional* arguments (PHP allows any user function to be called with more arguments than it declares, discarding the extras) are only accepted by functions that use one of the three argument-introspection constructs above. Every other user function keeps elephc's compile-time arity check, so `function f($a) {} f(1, 2);` is a compile error where PHP runs it.
- `serialize()`/`unserialize()` cover scalars, arrays, and objects (including the `__serialize`/`__unserialize`/`__sleep`/`__wakeup` magic methods and `r:`/`R:` object back-references) byte-for-byte compatibly with PHP. Objects are registered before property hydration, so self-references resolve correctly; unknown class names materialize as `__PHP_Incomplete_Class` and preserve their original wire name. Remaining gaps: the deprecated `Serializable` interface (`C:` wire form) is unsupported, writing a property of an unserialized object held in a `Mixed` does not persist (a separate `Mixed` property-write limitation), and `unserialize()` does not emit PHP's `E_WARNING` / `E_NOTICE` on malformed input — it just returns `false`.
- Reading a variable after a straight-line `unset()` of it is a compile error (`Undefined variable: $a`), in both modes, where PHP warns and evaluates the read as `null`. See [Local retyping](#local-retyping) above for the full unset/retype/mixed-storage mechanism and the `isset()`/`empty()`/`??` probes that stay legal on the unbound name.
- `defined('Class::CONST')` follows PHP visibility for public, protected, and private members, including lexical `self::`/`parent::`. Remaining gaps: `static::` is not late-bound (AOT currently returns `false`); `self::`/`parent::` outside a class currently return `false` instead of PHP's Error; trait-imported constants and eval-mode class-constant names are not resolved. Dynamic and first-class-callable `defined()` names are still rejected at compile time.

### Filesystem functions not implemented

These standard PHP filesystem functions are intentionally absent from elephc because they have no meaningful semantics in a compiled native binary:

- `move_uploaded_file()`, `is_uploaded_file()` — both rely on the PHP-FPM/SAPI request lifecycle (the `$_FILES` superglobal and a per-request "uploaded files" registry). A standalone compiled binary has no such request scope.
- `fgetss()` — deprecated in PHP 7.3 and removed in PHP 8.0. New code should use `strip_tags()` on the result of `fgets()`.

### Compiler diagnostics

elephc reports errors with source spans. Example:

```text
error[3:5]: Undefined variable: $name
error[8:1]: Function 'foo' declared return type string but returns int
```

The compiler also emits non-fatal warnings (unused variables, unreachable code).

### Runtime diagnostics

Runtime warnings flow through a suppressible diagnostics channel. The `@` operator hides those warnings for its operand only, while fatal runtime errors and compile-time diagnostics remain visible. Current suppressible warnings include `fopen()` / `file_get_contents()` open failures and duplicate `define()` calls.
