---
title: "Classes"
description: "Classes, interfaces, abstract classes, traits, enums, properties, and inheritance."
sidebar:
  order: 9
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
- signature-only methods and PHP 8.4 property hook contracts; method and hook bodies are not allowed in interfaces
- interface inheritance flattened transitively with cycle detection

Interface properties must be hooked contracts. A concrete class can satisfy a `{ get; }` contract with a public readable property, a `{ set; }` contract with a public writable property, or both with an invariant public property. Get-only contracts allow covariant concrete types; set-only contracts allow contravariant concrete types.

```php
<?php
interface HasName {
    public string $name { get; set; }
}

class Product implements HasName {
    public string $name = "widget";
}
```

### Built-in interfaces

The compiler injects the following interfaces, available without any
`implements` declaration on the user side:

| Interface | Methods |
|---|---|
| `Traversable` | (marker) |
| `Iterator` extends `Traversable` | `current(): mixed`, `key(): mixed`, `next(): void`, `valid(): bool`, `rewind(): void` |
| `IteratorAggregate` extends `Traversable` | `getIterator(): Traversable` |
| `OuterIterator` extends `Iterator` | `getInnerIterator(): ?Iterator` |
| `RecursiveIterator` extends `Iterator` | `getChildren(): ?RecursiveIterator`, `hasChildren(): bool` |
| `SeekableIterator` extends `Iterator` | `seek(int $offset): void` |
| `Countable` | `count(): int` |
| `ArrayAccess` | `offsetExists(mixed $offset): bool`, `offsetGet(mixed $offset): mixed`, `offsetSet(mixed $offset, mixed $value): void`, `offsetUnset(mixed $offset): void` |
| `SplObserver` | `update(SplSubject $subject): void` |
| `SplSubject` | `attach(SplObserver $observer): void`, `detach(SplObserver $observer): void`, `notify(): void` |
| `Stringable` | `__toString(): string` |
| `JsonSerializable` | `jsonSerialize(): mixed` |
| `Throwable` | `getMessage(): string`, `getCode(): int`, `getFile(): string`, `getLine(): int`, `getTrace(): array`, `getTraceAsString(): string`, `getPrevious(): ?Throwable`, `__toString(): string` |

`count($obj)` automatically dispatches to `Countable::count()` when
`$obj` is an instance of a class implementing `Countable`.

User classes cannot implement `Throwable` directly, matching PHP. Extend
`Exception` or `Error` instead; user interfaces may extend `Throwable`, and
classes that extend `Exception` or `Error` can implement those user interfaces.

Classes implementing `ArrayAccess` can use PHP subscript syntax:
`$obj[$key]` dispatches to `offsetGet()`, `$obj[$key] = $value` dispatches to
`offsetSet()`, `isset($obj[$key])` dispatches to `offsetExists()`, and
`unset($obj[$key])` dispatches to `offsetUnset()`.

`Serializable` is intentionally not provided: it is deprecated since
PHP 8.1. Use `__serialize` / `__unserialize` magic methods instead
(when those land).

### Built-in SPL containers and storage iterators

The SPL container and storage iterator classes are built-ins:
`SplDoublyLinkedList`, `SplStack`, `SplQueue`, `SplFixedArray`,
`EmptyIterator`, `InternalIterator`, `ArrayIterator`, `ArrayObject`, `IteratorIterator`,
`LimitIterator`, `NoRewindIterator`, `InfiniteIterator`, `FilterIterator`,
`CallbackFilterIterator`, `CachingIterator`, `AppendIterator`,
`MultipleIterator`, `RecursiveArrayIterator`, `RecursiveFilterIterator`,
`RecursiveCallbackFilterIterator`, `RecursiveIteratorIterator`, and
`ParentIterator`. They participate in `class_exists()`,
`get_declared_classes()`, `instanceof`, inherited class constants, interface
checks, `foreach`, and `ArrayAccess` where PHP expects it. PHP does not include
`InternalIterator` in `spl_classes()`, so elephc keeps it out of that helper too.

| Class | Parent | Interfaces |
|---|---|---|
| `SplDoublyLinkedList` | — | `Iterator`, `Countable`, `ArrayAccess` |
| `SplStack` | `SplDoublyLinkedList` | inherited from parent |
| `SplQueue` | `SplDoublyLinkedList` | inherited from parent |
| `SplFixedArray` | — | `IteratorAggregate`, `ArrayAccess`, `Countable`, `JsonSerializable` |
| `EmptyIterator` | — | `Iterator` |
| `InternalIterator` | — | `Iterator` |
| `ArrayIterator` | — | `Iterator`, `ArrayAccess`, `SeekableIterator`, `Countable` |
| `ArrayObject` | — | `IteratorAggregate`, `ArrayAccess`, `Countable` |
| `IteratorIterator` | — | `OuterIterator` |
| `LimitIterator` | `IteratorIterator` | inherited from parent |
| `NoRewindIterator` | `IteratorIterator` | inherited from parent |
| `InfiniteIterator` | `IteratorIterator` | inherited from parent |
| `FilterIterator` | `IteratorIterator` | inherited from parent |
| `CallbackFilterIterator` | `FilterIterator` | inherited from parent |
| `CachingIterator` | `IteratorIterator` | `ArrayAccess`, `Countable`, `Stringable` |
| `AppendIterator` | `IteratorIterator` | inherited from parent |
| `MultipleIterator` | — | `Iterator` |
| `RecursiveArrayIterator` | `ArrayIterator` | `RecursiveIterator` |
| `RecursiveFilterIterator` | `FilterIterator` | `RecursiveIterator` |
| `RecursiveCallbackFilterIterator` | `CallbackFilterIterator` | `RecursiveIterator` |
| `RecursiveIteratorIterator` | — | `OuterIterator` |
| `ParentIterator` | `RecursiveFilterIterator` | inherited from parent |

See [SPL](spl.md) for the supported method surface, iterator modes, examples,
and current compatibility gaps.

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

### Abstract properties

An abstract class may declare a PHP 8.4 hooked property contract as `abstract`. The declaration has no default value or hook body, and every concrete subclass must redeclare the property with a compatible public/protected property. Static, final, private, and `readonly` hooked abstract properties are rejected.

```php
<?php
abstract class Shape {
    abstract public int $sides { get; set; }
}

class Square extends Shape {
    public int $sides = 4;
}
```

The concrete redeclaration reuses the parent's slot (offsets are stable across the inheritance chain), so the property is accessible to both parent and child methods. elephc supports hook contracts (`{ get; }`, `{ set; }`, and `{ get; set; }`) in abstract classes, interfaces, and traits; executable hook bodies are not implemented yet.

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
- `readonly class` makes all instance properties readonly; static properties stay mutable

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

Property type declarations are checked at compile time for both instance and static properties. Defaults and later assignments must be compatible with the declared type, including constructor assignments through untyped parameters. Typed properties without an explicit default start in PHP's uninitialized state; reading an instance or static property before the first assignment is a fatal runtime error, while assigning values such as `0`, `false`, `""`, or `null` to compatible nullable storage initializes the slot normally. Nullable shorthand (`?T`) and union storage use the compiler's boxed mixed representation internally. `void` and `callable` property types are rejected.

Property default values are applied both for the normal `new ClassName()` form and for dynamic `new $variable()` instantiation (and therefore for runtime-instantiated stream wrappers and stream filters). When the class name resolves to a known class, dynamic instantiation follows the same allocation path as direct construction, so constructor arguments are evaluated and `__construct` runs normally.

### Property redeclaration

A child class may redeclare a property inherited from a non-private parent. The redeclaration is checked at compile time and must follow PHP rules:

- Visibility cannot be reduced (`public` → `protected` is rejected; `protected` → `public` is allowed).
- Declared types are invariant. A typed parent property must be redeclared with the same type. A typed parent property cannot become untyped, and an untyped parent property cannot gain a type in the child.
- `readonly` is monotonic — a `readonly` parent property must stay `readonly` in the child. A non-readonly parent property may become `readonly` in the child.
- The by-reference qualifier on a property cannot change across inheritance.
- `final` parent properties cannot be redeclared.
- The child shares the parent's slot, so reads of the property from inherited methods see the child's value.

```php
<?php
class Base {
    public int $value = 0;
}

class Child extends Base {
    public int $value = 5;
}

echo (new Child())->value; // 5
```

Private parent properties are still considered separate slots in PHP, but elephc rejects same-named redeclarations through them; declare a different name in the child for now.

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

Supported receivers are `ClassName::$prop`, `self::$prop`, `parent::$prop`, and `static::$prop`. Static property visibility and declared types are checked at compile time. Typed static properties without defaults use the same uninitialized-read fatal as typed instance properties. Inherited static properties share the declaring class storage until a subclass redeclares the property. Redeclarations follow PHP rules: non-private inherited properties keep invariant declared types, cannot reduce visibility, and cannot override `final` properties. Private static properties redeclared in subclasses are independent slots; `static::$prop` is still late-bound and reports a fatal runtime error if the current method scope cannot access the matched private slot.

Static properties in elephc, like in PHP, are always mutable — even on a `readonly class`. PHP's `readonly` modifier only constrains instance properties; declaring `public readonly static` is a compile error in both PHP and elephc.

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

By-reference promoted parameters may also have defaults. If no argument is passed, elephc creates a private reference cell for the default value; if a variable is passed, the promoted property aliases that variable. `readonly` by-reference promoted properties are rejected at compile time because construction would have to bind an indirect mutable alias to a readonly slot.

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
$segment = "profile";
echo $user?->{$segment}?->name ?? "anonymous";
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

## Dynamic instantiation (`new $variable()`)

`new $variable()` constructs an instance whose class is selected at runtime from a string variable:

```php
<?php
class Foo {}
class Bar {}

$cls = "Foo";
$obj = new $cls();                       // Foo instance
echo gettype($obj);                      // "object"

$missing = "NoSuchClass";
$bad = new $missing();                   // PHP null
echo gettype($bad);                      // "NULL"
```

elephc resolves the class name case-insensitively against compile-time class metadata, matching PHP class lookup. A match dispatches through the same allocation path as `new ClassName()`, including constructor calls, declared property defaults, and supported built-in/SPL runtime storage initialization. An unknown name currently yields PHP `null`; the missing-class fatal path is not yet tightened.

## Override rules
Same parameter count, same pass-by-reference positions, same default layout, same variadic shape.

## Traits
Flattened at compile time. Support: `use Trait;`, multiple traits, `insteadof`, `as`, trait properties, static trait methods.

Traits may declare abstract hooked property contracts. A concrete class using the trait must satisfy the contract directly or inherit it through an abstract base class that is later completed by a concrete child.

## Property access
`->` for properties and methods.

### Dynamic property access

The property name can be computed at runtime with the `$obj->{$expr}` syntax,
where `$expr` is any expression that evaluates to a string. The same form works
as an assignment target and combines with the nullsafe operator (`$obj?->{$expr}`).

```php
<?php
class Point {
    public int $x = 1;
    public int $y = 2;
}

$p = new Point();
$field = "x";
echo $p->{$field};        // 1
$p->{$field} = 9;
echo $p->x;               // 9
```

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

### Built-in `SortDirection`

PHP 8.6's global unit enum is available without a user declaration:

```php
<?php
function sqlSortKeyword(SortDirection $direction): string {
    return match ($direction) {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    };
}

echo sqlSortKeyword(SortDirection::Descending); // DESC
```

`SortDirection` has two cases, `Ascending` and `Descending`, no backing value, and works with enum case identity, `SortDirection::cases()`, `enum_exists()`, type declarations, `match`, imports, and fully-qualified `\SortDirection` references.

## Magic methods
- `__toString()` — string coercion
- `__get($name)` — reading undefined property
- `__set($name, $value)` — writing undefined property
- `__invoke(...$args)` — calling an object directly
- `__call($name, $args)` — intercepting missing instance methods

## Attributes

PHP 8.0 attributes (`#[Name]`) decorate declarations. elephc parses attributes at every site PHP allows: classes, interfaces, traits, enums, enum cases, top-level functions, methods, properties, function/method/closure parameters (incl. promoted constructor params), closures, and arrow functions. Class, method, and property attributes have limited runtime reflection through the helpers below; attributes on other declaration sites are currently validated for syntax and kept only in the AST.

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

User-defined attributes (e.g. `#[Author]`, `#[Pure]`, `#[Memoized]`) parse and persist in the AST. They have no compile-time semantics, but their **names** and positional **literal arguments** are reachable at runtime through lightweight helper builtins and the supported Reflection API:

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

For a more PHP-idiomatic API, `class_get_attributes()` and `ReflectionClass::getAttributes()` return the same data wrapped as `ReflectionAttribute` instances:

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

Reflection is also available for class members:

```php
<?php
class Controller {
    #[Route("/home", "GET")]
    public function index() {}

    #[Column("id")]
    public int $id = 0;
}

$class = new ReflectionClass(Controller::class);
echo $class->getAttributes()[0]->getName();

$method = new ReflectionMethod('Controller', 'index');
echo $method->getAttributes()[0]->getArguments()[0]; // /home

$property = new ReflectionProperty('Controller', 'id');
echo $property->getAttributes()[0]->getName(); // Column
```

`ReflectionAttribute` is a final synthetic built-in class with `getName(): string`, `getArguments(): array`, and `newInstance(): mixed` methods. It is populated internally by `class_get_attributes()` and the supported Reflection lookups and cannot be constructed or populated directly from user code; its metadata slots are private. `newInstance()` constructs the attribute class on demand when the attribute class exists in the program and the captured arguments are supported literals:

```php
<?php
class Route {
    public function __construct(string $path) {
        echo $path;
    }
}

#[Route("/lazy")]
class Controller {}

$attr = (new ReflectionClass('Controller'))->getAttributes()[0];
$instance = $attr->newInstance(); // constructor runs here
echo ($instance instanceof Route) ? "yes" : "no";
```

| Function | Signature | Description |
|---|---|---|
| `class_attribute_names()` | `class_attribute_names($class_name): array` | Return the resolved attribute names decorating the class |
| `class_attribute_args()` | `class_attribute_args($class_name, $attribute_name): array` | Return the supported literal positional arguments for the first matching class attribute |
| `class_get_attributes()` | `class_get_attributes($class_name): array` | Return `ReflectionAttribute` objects for the class attributes |

| Reflection method | Supported constructor | Description |
|---|---|---|
| `ReflectionClass::getName()` | `new ReflectionClass($class_name)` | Return the resolved class name |
| `ReflectionClass::getAttributes()` | `new ReflectionClass($class_name)` | Return `ReflectionAttribute` objects for class attributes |
| `ReflectionMethod::getAttributes()` | `new ReflectionMethod($class_name, $method_name)` | Return `ReflectionAttribute` objects for method attributes |
| `ReflectionProperty::getAttributes()` | `new ReflectionProperty($class_name, $property_name)` | Return `ReflectionAttribute` objects for property attributes |
| `ReflectionAttribute::newInstance()` | Internal only | Instantiate the attribute class from captured literal args |

Limitations today:
- All arguments to `class_attribute_names()`, `class_attribute_args()`, `class_get_attributes()`, and `new ReflectionClass/Method/Property(...)` must be compile-time class/member strings. `ClassName::class` is accepted for the class-name argument of `new ReflectionClass/Method/Property(...)`, and normal named-argument / static associative-spread normalization runs before the literal-string check. Dynamic class, method, property, or attribute names require a runtime name→id lookup table that is not yet implemented.
- Only **literal** positional arguments are materialized by reflection helpers today (string, int, bool, null, plus `-N` for negative ints). Other legal PHP attribute arguments can still be parsed and compiled, and `class_attribute_names()` can still list the attribute name, but `class_attribute_args()`, `class_get_attributes()`, and Reflection `getAttributes()` report an error if they would need unsupported argument metadata.
- When several attributes share a name on the same class, `class_attribute_args()` returns the args of the first match; `class_get_attributes()` does expose every occurrence as a separate `ReflectionAttribute` in source order.
- `ReflectionClass` supports `getName()` and `getAttributes()`. `ReflectionMethod` and `ReflectionProperty` currently support `getAttributes()` only; broader APIs such as `getProperties()`, `getMethods()`, and object construction through `ReflectionClass::newInstance()` are not yet available.

### Class constants

```php
<?php
class Math {
    const PI = 314;
    public const E = 271;
    const TAU = self::PI * 2;
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

Class constants (PHP 7.1+ visibility, PHP 8.1+ `final`) live on classes, interfaces, and traits. They are inherited from parents and implemented interfaces (transitively). At codegen time elephc inlines the constant's foldable value at every access site — there is no runtime lookup. Class constant expressions may reference other class constants through `ClassName::CONST`, `self::CONST`, or `parent::CONST`; `self::class` and `parent::class` are also accepted. `self::` and `parent::` are early-bound to the declaring class, matching PHP. `static::CONST` is rejected in class constant expressions because PHP does not allow late-static binding in compile-time constants. Attributes on class constants are accepted and stored in the AST.

## Limitations
- `readonly static` properties are rejected to match PHP. Static properties in a `readonly class` are still mutable.
- Property hook bodies are not implemented; elephc supports hook contracts only.
- Shadowing a private parent property with a same-named child property is not yet supported (PHP gives them separate slots; elephc uses one slot per name)
- Class constants must be literal-or-foldable expressions; cyclic constant references are not supported.
- Anonymous classes (`new class { ... }`) are not yet supported.
- Class attribute names and supported literal args are exposed at runtime through `class_attribute_names()`, `class_attribute_args()`, `class_get_attributes()`, and the supported `ReflectionClass`/`ReflectionMethod`/`ReflectionProperty::getAttributes()` APIs; parameter reflection is not yet available. `#[\Override]`, `#[\Deprecated]`, and `#[\AllowDynamicProperties]` are enforced/diagnosed/honored at compile time and runtime; `#[\SensitiveParameter]` is parsed but not yet propagated to parameters (refactor of param representation and stack-trace infrastructure pending).
