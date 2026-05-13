---
title: "Classes"
description: "Classes, interfaces, abstract classes, traits, enums, properties, and inheritance."
sidebar:
  order: 8
---

## Class declaration
```php
<?php
class Point {
    public $x;
    public $y;

    public function __construct($x, $y) {
        $this->x = $x;
        $this->y = $y;
    }

    public function magnitude() {
        return sqrt($this->x * $this->x + $this->y * $this->y);
    }

    public static function origin() {
        return new Point(0, 0);
    }
}
```

Class, interface, trait, and method lookup is case-insensitive like PHP:
`new point()`, `POINT::origin()`, and `$p->MAGNITUDE()` resolve to `Point` and
its declared methods. Object properties remain case-sensitive, so `$p->x` and
`$p->X` are distinct property names.

## Interfaces
```php
<?php
interface Named {
    public function name();
}

class Product implements Named {
    public function name() { return "widget"; }
    public function label() { return strtoupper($this->name()); }
}
```
- signature-only methods, no bodies, no properties
- interface inheritance flattened transitively with cycle detection

## Type checks with instanceof
```php
<?php
interface Renderable {
    public function render();
}

class Widget {
    public function render() { return "widget"; }
}

class Button extends Widget implements Renderable {}

$item = new Button();
echo ($item instanceof Button) ? "yes" : "no";      // yes
echo ($item instanceof Widget) ? "yes" : "no";      // yes
echo ($item instanceof Renderable) ? "yes" : "no";  // yes

$target = "Button";
echo ($item instanceof $target) ? "yes" : "no";     // yes
```

The runtime check uses emitted class metadata, so subclasses match parent classes and implemented interfaces. The left-hand side may be a direct object or a boxed `mixed` / nullable / union value; non-object payloads return `false` once any dynamic target has been validated. Supported targets are named classes/interfaces, `self`, `parent`, late-bound `static`, dynamic class/interface strings, and dynamic object expressions.

## Abstract classes
```php
<?php
abstract class BaseGreeter {
    abstract public function label();
    public function greet() { return "hi " . $this->label(); }
}
```
- cannot be instantiated
- abstract methods must be bodyless
- non-abstract classes may not have abstract methods

## Final classes, methods, and properties
```php
<?php
final class InvoiceNumber {
    final public $value = 42;

    final public function label() {
        return "invoice:" . $this->value;
    }
}
```
- `final class` cannot be extended
- `final` methods cannot be overridden by subclasses
- `final` properties cannot be redeclared by subclasses
- `final` does not change object layout or dispatch for normal calls
- `abstract final` classes and methods are rejected
- `final private` methods emit a warning, matching PHP, because private methods are not overridden; `__construct` is the exception
- `final private` properties are rejected, matching PHP

## Properties
- `public`, `protected`, `private` visibility
- Optional default values
- Optional type declarations, for example `public int $id` or `public ?string $email = null`
- `readonly` properties (only assigned in `__construct`)
- `final` properties, which can be read normally but cannot be redeclared by subclasses
- Static properties with `public static`, `protected static`, or `private static`, including typed static properties
- `readonly class` makes all properties readonly

```php
<?php
class User {
    public int $id;
    public string $name = "Ada";
    public ?string $email = null;

    public function __construct($id) {
        $this->id = $id;
    }
}
```

Property type declarations are checked at compile time for both instance and static properties. Defaults and later assignments must be compatible with the declared type, including constructor assignments through untyped parameters. Nullable shorthand (`?T`) and union storage use the compiler's boxed mixed representation internally. `void` and `callable` property types are rejected.

## Static properties
Static properties use class-scoped storage and are accessed with `::`.

```php
<?php
class Counter {
    public static int $count = 1;

    public static function bump() {
        self::$count = self::$count + 1;
        return self::$count;
    }
}

echo Counter::$count; // 1
Counter::$count = 5;
echo Counter::bump(); // 6
```

Supported receivers are `ClassName::$prop`, `self::$prop`, `parent::$prop`, and `static::$prop`. Static property visibility and declared types are checked at compile time. Inherited static properties share the declaring class storage until a subclass redeclares the property. Redeclarations follow PHP rules: non-private inherited properties keep invariant declared types, cannot reduce visibility, and cannot override `final` properties. Private static properties redeclared in subclasses are independent slots; `static::$prop` is still late-bound and reports a fatal runtime error if the current method scope cannot access the matched private slot.

Static array properties support direct element writes:

```php
<?php
class Registry {
    public static array $items = [];
}

Registry::$items[] = 10;
Registry::$items[0] = 12;
echo Registry::$items[0]; // 12
```

## Constructor
Called automatically with `new`:
```php
$p = new Point(3, 4);
```

Constructor property promotion is supported. Visibility or `readonly` before a constructor parameter declares a property and assigns the incoming argument at the start of `__construct`.

```php
<?php
class User {
    public function __construct(
        public int $id,
        private string $name = "Ada",
        readonly ?int $rank = null
    ) {}

    public function name() {
        return $this->name;
    }
}

$user = new User(7);
echo $user->id;      // 7
echo $user->name();  // Ada
```

Promoted properties support `public`, `protected`, `private`, `readonly`, nullable and union type declarations, constructor parameter defaults, and by-reference parameters. Variadic promotion is rejected, matching PHP.

By-reference promoted properties are supported when the constructor argument is a variable:

```php
<?php
class Counter {
    public function __construct(public int &$value) {}
}

$value = 1;
$counter = new Counter($value);

$value = 2;
echo $counter->value;  // 2

$counter->value = 3;
echo $value;           // 3
```

Current limitations for by-reference promotion: the promoted property cannot be `readonly`, and by-reference promoted parameters cannot use default values yet.

## Instance methods and $this
Virtual dispatch for overrides.
Private methods are not virtual.

## Nullsafe access
Use `?->` when a receiver may be `null`:

```php
<?php
echo $user?->profile?->name ?? "anonymous";
echo $user?->profile?->label() ?? "missing";
echo $user?->profile->address?->city ?? "unknown";
```

When a nullsafe receiver is `null`, elephc skips the rest of that postfix chain and returns `null`. This matches PHP for mixed chains such as `$user?->profile->address`: the ordinary `->address` segment is skipped when `$user` is `null`, but still warns or fatals normally if `$user` is non-null and `profile` itself is `null`. Method arguments, array indexes, and callable arguments on the skipped branch are not evaluated.

## parent::method()
Direct parent implementation call.

## self::method()
Binds to lexical class, not runtime child.

## static::method()
Late static binding — resolves against called class at runtime.

## Static methods
Called with `::`, no `$this`.

## Class name reflection (`::class`)

`::class` returns the fully-qualified class name as a string at compile time.

```php
<?php
namespace App;
class Logger {
    public static function tag() {
        return self::class;          // "App\Logger"
    }
}
echo Logger::class;                  // "App\Logger"
echo \App\Logger::class;             // "App\Logger"
```

Supported receivers: `Class::class`, `\Vendor\Class::class`, `self::class`, `parent::class`, `static::class`.

`static::class` follows PHP late static binding and resolves to the called class.
For named receivers, elephc preserves PHP's written/imported spelling for the
`::class` string while still using case-insensitive class lookup for executable
operations such as `new`, `instanceof`, static method calls, and static property
access.

## Late static binding constructors (`new self()`, `new static()`, `new parent()`)

The `new self()`, `new static()`, and `new parent()` factory patterns are supported inside class methods:

```php
<?php
class Box {
    public string $label = "default";
    public static function make(): Box {
        return new self();
    }
}
$b = Box::make();
echo $b->label;                      // "default"

class Base {
    public string $kind = "base";
}

class Child extends Base {
    public static function makeBase(): Base {
        return new parent();
    }
}
```

`new static()` follows PHP late static binding and constructs an instance of the called class.

## Override rules
Same parameter count, same pass-by-reference positions, same default layout, same variadic shape.

## Traits
Flattened at compile time. Support: `use Trait;`, multiple traits, `insteadof`, `as`, trait properties, static trait methods.

## Property access
`->` for properties and methods.

## Enums
```php
<?php
enum Color: int {
    case Red = 1;
    case Green = 2;
}
echo Color::Red->value;          // 1
echo Color::from(2) === Color::Green; // 1
```
Pure and backed enums. `->value`, `::from()`, `::tryFrom()`, `::cases()`. Only `int` and `string` backing types.

## Magic methods
- `__toString()` — string coercion
- `__get($name)` — reading undefined property
- `__set($name, $value)` — writing undefined property

## Attributes

PHP 8.0 attributes (`#[Name]`) decorate declarations. elephc parses attributes at every site PHP allows: classes, interfaces, traits, enums, enum cases, top-level functions, methods, properties, function/method/closure parameters (incl. promoted constructor params), closures, and arrow functions. Class-level attributes have limited runtime reflection through the helpers below; attributes on other declaration sites are currently validated for syntax and kept only in the AST.

```php
<?php
#[Author("Ada"), Version(1)]
class Greeter {
    #[Slot]
    public string $who;

    public function __construct(#[Required] string $who) {
        $this->who = $who;
    }

    #[Pure]
    public function greet(): void { echo "Hello"; }
}

class LoudGreeter extends Greeter {
    #[\Override]
    public function greet(): void { echo "HELLO"; }
}

$pure = #[Pure] fn (int $x) => $x + 1;

#[Memoized]
function double(int $x): int { return $x * 2; }
```

Supported syntax:
- single attribute: `#[Foo]`
- attribute with arguments: `#[Bar(1, "two")]`
- multiple attributes per group: `#[A, B(1)]`
- stacked groups: `#[A] #[B]`
- fully-qualified names: `#[\Symfony\Contracts\Service\Attribute\Required]`

`#` outside an attribute group introduces a PHP-style line comment, identical to `//`. Attributes before non-declaration statements (`echo`, `if`, assignments) are rejected — PHP's strict rule.

### Compile-time enforced attributes

- **`#[\Override]`** (PHP 8.3) — the type checker verifies that the marked method actually overrides a method declared in a parent class or implemented interface (transitively). A typo in the method name or a missing parent method becomes a compile-time error: `<class>::<method>() has #[\Override] attribute, but no matching parent method was found`. Both the unqualified `#[Override]` and fully-qualified `#[\Override]` forms are recognized.
- **`#[\Deprecated]`** / **`#[\Deprecated("reason")]`** (PHP 8.4) — calls to the marked function, method, or static method emit a compile warning: `Call to deprecated function: name() — reason`. The reason argument (if a string literal) is appended to the message.
- **`#[\AllowDynamicProperties]`** (PHP 8.2) — instances of the marked class accept assignment of undeclared properties at runtime. Each instance carries a per-object hashtable side-table allocated by the constructor (~296 bytes); the type checker accepts undeclared reads as `mixed`. The hashtable is freed automatically with the object.

Built-in attributes follow PHP class-name resolution. In a namespace, `#[Deprecated]` means `#[CurrentNamespace\Deprecated]`; use `#[\Deprecated]` or an import alias such as `use Deprecated as Old; #[Old]` to target the global built-in attribute.

```php
<?php
#[\AllowDynamicProperties]
class Bag {
    public int $declared = 1;
}

$b = new Bag();
$b->extra = 42;          // accepted, stored in side-table
$b->name = "elephc";     // heterogeneous values supported
echo $b->declared;        // 1
echo $b->extra;           // 42
echo $b->name;            // "elephc"
echo $b->missing;         // empty (Mixed null)
```

User-defined attributes (e.g. `#[Author]`, `#[Pure]`, `#[Memoized]`) parse and persist in the AST. They have no compile-time semantics, but their **names** and positional **literal arguments** are reachable at runtime through the `class_attribute_names()` and `class_attribute_args()` builtins:

```php
<?php
#[Author("Ada"), Version(1)]
class Greeter {}

#[\Override]
class Solo {}

#[Route("/api/users", "GET", true)]
class UserController {}

foreach (class_attribute_names('Greeter') as $name) {
    echo $name, "\n";
}
// Author
// Version

echo class_attribute_names('Solo')[0]; // "Override" (resolved name)

foreach (class_attribute_args('UserController', 'Route') as $arg) {
    echo $arg, "\n";
}
// /api/users
// GET
// 1     ← `true` echoes as 1 in PHP
```

`class_attribute_args()` returns an `array<mixed>` whose elements preserve their original PHP type — strings stay strings, ints stay ints, booleans stay booleans, and `null` is `null`. The args are interned at compile time and boxed into mixed cells on demand at the call site.

For a more PHP-idiomatic API, `class_get_attributes()` returns the same data wrapped as `ReflectionAttribute` instances:

```php
<?php
#[Author("Ada", 1815), Version("1.0", true)]
class Greeter {}

foreach (class_get_attributes('Greeter') as $attr) {
    echo $attr->getName(), ": ";
    foreach ($attr->getArguments() as $arg) {
        echo "[", $arg, "]";
    }
    echo "\n";
}
// Author: [Ada][1815]
// Version: [1.0][1]
```

`ReflectionAttribute` is a final synthetic built-in class with `getName(): string` and `getArguments(): array` methods. It is populated internally by `class_get_attributes()` and cannot be constructed or populated directly from user code; its metadata slots are private.

Limitations today:
- All arguments to `class_attribute_names()`, `class_attribute_args()`, and `class_get_attributes()` must be **string literals** at the call site — dynamic class or attribute names (variables) require a runtime name→id lookup table that is not yet implemented.
- Only **literal** positional arguments are materialized by reflection helpers today (string, int, bool, null, plus `-N` for negative ints). Other legal PHP attribute arguments can still be parsed and compiled, and `class_attribute_names()` can still list the attribute name, but `class_attribute_args()` / `class_get_attributes()` report an error if they would need unsupported argument metadata.
- When several attributes share a name on the same class, `class_attribute_args()` returns the args of the first match; `class_get_attributes()` does expose every occurrence as a separate `ReflectionAttribute` in source order.
- The full `ReflectionClass` API (`getProperties()`, `getMethods()`, `newInstance()`, …) is not yet available.

### Class constants

```php
<?php
class Math {
    const PI = 314;
    public const E = 271;
}
echo Math::PI;        // 314
echo self::PI;        // inside Math methods

interface Limits {
    const MAX = 100;
}
class Bound implements Limits {
    public function get(): int { return Limits::MAX; }
}
```

Class constants (PHP 7.1+ visibility, PHP 8.1+ `final`) live on classes, interfaces, and traits. They are inherited from parents and implemented interfaces (transitively). At codegen time elephc inlines the constant's literal value at every access site — there is no runtime lookup, and recursive constant references (`const FOO = self::BAR + 1`) are not yet supported. Attributes on class constants are accepted and stored in the AST.

## Limitations
- No abstract properties
- No `readonly static` properties
- No `readonly` or default-valued by-reference promoted properties
- No instance property redeclaration across inheritance chain
- Class constants must be literal-or-foldable expressions; `self::OTHER + 1` style recursive references are not supported.
- Anonymous classes (`new class { ... }`) are not yet supported.
- Class attribute names and supported literal args are exposed at runtime through `class_attribute_names()`, `class_attribute_args()`, and `class_get_attributes()`; method/property/parameter reflection and `ReflectionClass` are not yet available. `#[\Override]`, `#[\Deprecated]`, and `#[\AllowDynamicProperties]` are enforced/diagnosed/honored at compile time and runtime; `#[\SensitiveParameter]` is parsed but not yet propagated to parameters (refactor of param representation and stack-trace infrastructure pending).
