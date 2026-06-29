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

The concrete redeclaration reuses the parent's slot (offsets are stable across the inheritance chain), so the property is accessible to both parent and child methods. elephc supports hook contracts (`{ get; }`, `{ set; }`, and `{ get; set; }`) in abstract classes, interfaces, and traits, and executable hook bodies on concrete properties (see [Property hooks](#property-hooks-get--set)).

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

An `array`-typed (or untyped) property may take an associative literal default such as `['a' => 1]`. The property is then stored as an associative array, so string-key reads and writes (`$this->data['a']`, `$this->data[$key]`) type-check and run like any other associative array. A positional literal default (`[1, 2, 3]`) keeps integer-keyed list storage.

```php
<?php
class Bag {
    public array $data = ['a' => 1, 'b' => 2];
    public function get(string $key): int {
        return $this->data[$key] ?? 0;
    }
}
echo (new Bag())->get('a'); // 1
```

### Asymmetric visibility (`private(set)`)

PHP 8.4 asymmetric visibility lets a property be read more widely than it can be written. A `(set)` modifier after a visibility keyword sets the write visibility independently of the read visibility:

```php
<?php
class Counter {
    public private(set) int $value = 0;     // read: public, write: private

    public function increment(): void {
        $this->value = $this->value + 1;     // allowed: write from inside the class
    }
}

$c = new Counter();
$c->increment();
echo $c->value;   // 2 — public read
// $c->value = 9; // rejected: write is private
```

Rules:

- The `set` visibility applies to writes; the ordinary (read) visibility applies to reads.
- A lone `private(set)` / `protected(set)` leaves the read visibility at its `public` default.
- `protected(set)` allows writes from the declaring class and its subclasses; `private(set)` only from the declaring class.
- The write visibility must not be weaker than the read visibility (`private public(set)` is rejected).
- The property must be typed, and the modifier is not allowed on static properties.
- Indirect writes through an array element (`$obj->items[] = x`, `$obj->items['k'] = x`) are writes too, so they honor the `set` visibility — not the (wider) read visibility.

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

### Property hooks (`get` / `set`)

A property can define **hooks** that run when it is read or written, replacing hand-written getter and setter methods. The hooks live in a `{ ... }` block after the property name. Reading the property runs its `get` hook; assigning to it runs its `set` hook.

A `get`-only property is **virtual** — it has no stored value of its own and is read-only. The short form `get => expr;` returns `expr`; the block form `get { ... }` runs statements and returns a value:

```php
<?php
class Person {
    public string $first = "Ada";
    public string $last = "Lovelace";

    public string $full {
        get => $this->first . " " . $this->last;
    }
}

echo (new Person())->full; // Ada Lovelace
```

A property with both `get` and `set` typically reads from and writes to a separate backing field. The value assigned to the property is available inside the `set` hook as `$value` (rename it with `set(Type $other)`):

```php
<?php
class Thermostat {
    private float $c = 0.0;

    public float $celsius {
        get => $this->c;
        set { $this->c = $value; }
    }

    public float $fahrenheit {
        get => $this->c * 9.0 / 5.0 + 32.0;
        set { $this->c = ($value - 32.0) * 5.0 / 9.0; }
    }
}

$t = new Thermostat();
$t->fahrenheit = 212.0;
echo $t->celsius; // 100
```

Inside a property's own hook, `$this->prop` accesses the raw stored value rather than re-running the hook, so a hook may read and write the property it belongs to (a *backed* property):

```php
<?php
class Name {
    public string $value {
        get => $this->value;
        set { $this->value = trim($value); }
    }
}

$n = new Name();
$n->value = "  Ada  ";
echo $n->value; // Ada
```

Rules:

- A property with a `get` hook but no `set` hook is read-only. Writing it is a compile-time error.
- Hooked properties cannot have a default value and cannot be `static`, `final`, or `readonly`.
- Each hook may be declared at most once per property.
- The short `set => expr;` form is not supported; use a block `set { ... }`.
- Hooks are inherited by subclasses along with the property.

Abstract classes, interfaces, and traits may declare hook *contracts* (`{ get; }`, `{ set; }`, `{ get; set; }`) with no body; see [Abstract properties](#abstract-properties) and [Interfaces](#interfaces).

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

## Relative class types (`self`, `static`, `parent`)

`self`, `static`, and `parent` may be used as type declarations on method parameters, method return types, and properties. They resolve to the enclosing class (`self`, `static`) or its parent (`parent`):

```php
<?php
class Money {
    public function __construct(public int $amount) {}

    // `self` return type: enables fluent chaining.
    public function add(self $other): self {
        return new Money($this->amount + $other->amount);
    }
}

class Node {
    public ?self $next = null;     // a same-class (nullable) property
    public function __construct(public int $value) {}
}

trait Fluent {
    // In a trait, `static` resolves to the class that uses the trait.
    public function copy(): static {
        return clone $this;
    }
}
```

Rules:

- `self` and `static` resolve to the class the member is declared in; `parent` resolves to that class's parent.
- They are accepted in parameter, return, and property type positions, and may be combined with the nullable shorthand (`?self`) or unions (`self|null`).
- Used inside a trait, `self`/`static` resolve to the class that uses the trait, not the trait itself.
- Using `self`, `static`, or `parent` as a type outside of a class is rejected.
- For type checking, `static` is treated as the declaring class. A `static` return type chained directly on its declaring class works as expected; when a `static`-returning method is inherited and called on a subclass, the result is typed as the declaring class rather than the subclass.

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

## Dynamic method and static calls

A method or static method can be called by a name held in a variable:

```php
<?php
class Calculator {
    public function add(int $a, int $b): int { return $a + $b; }
    public static function version(): string { return "1.0"; }
}

$calc = new Calculator();
$method = "add";
echo $calc->$method(2, 3);        // 5 — dynamic instance method
echo $calc->{$method}(2, 3);      // 5 — brace form

$class = "Calculator";
echo $class::version();           // 1.0 — dynamic static call
$static = "version";
echo $class::$static();           // 1.0 — both class and method dynamic
```

`$obj->$name(...)` and `$class::$name(...)` are equivalent to `call_user_func([$obj, $name], ...)` / `call_user_func([$class, $name], ...)`. A dynamic method name on a literal class also works (`ClassName::$name(...)`). Arguments are forwarded positionally. A nullsafe dynamic method call (`$obj?->$name()`) is not yet supported, and **named arguments are rejected** in dynamic calls because the target method — and therefore its parameter names — is not known at compile time.

## Anonymous classes (`new class {}`)

An anonymous class defines and instantiates a class in one expression. It may take constructor arguments, extend a class, and implement interfaces:

```php
<?php
interface Logger {
    public function log(string $message): string;
}

function make_logger(string $prefix): Logger {
    return new class($prefix) implements Logger {
        public function __construct(private string $prefix) {}

        public function log(string $message): string {
            return $this->prefix . ": " . $message;
        }
    };
}

echo make_logger("INFO")->log("started"); // INFO: started
```

Rules:

- Constructor arguments go before any `extends`/`implements` clause: `new class(args) extends P implements I { ... }`.
- Each anonymous class is compiled to its own uniquely-named class, so two `new class {}` expressions produce two independent types.
- Like a named class, an anonymous class does **not** capture variables from the enclosing scope; pass data in through the constructor.
- `new readonly class { ... }` is supported.

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
echo Color::Red->name;           // Red
echo Color::Red->value;          // 1
echo Color::from(2) === Color::Green; // 1
```
Pure and backed enums. Every case exposes the read-only `->name` property (the case identifier); backed cases also expose `->value`. Plus `::from()`, `::tryFrom()`, `::cases()`. Only `int` and `string` backing types.

### Enum methods, constants, and interfaces

Enums may declare instance methods, static methods, constants, and an `implements` clause. Instance methods dispatch on the case singleton, so `$this` is the case:

```php
<?php
interface HasLabel {
    public function label(): string;
}

enum Suit: string implements HasLabel {
    case Hearts = "H";
    case Spades = "S";

    const COUNT = 2;

    public function label(): string {
        return match ($this) {
            Suit::Hearts => "Hearts",
            Suit::Spades => "Spades",
        };
    }

    public function code(): string {
        return $this->value;          // backing value
    }

    public static function default(): self {
        return Suit::Hearts;          // static factory
    }
}

echo Suit::Hearts->label();           // Hearts
echo Suit::default()->code();         // H
```

Rules:

- Instance methods may use `$this` (the case), `match ($this)`, the case `$this->name`, the backing `$this->value`, and `self::CONST`.
- Static methods dispatch like class static methods and can act as factories.
- An enum can `implements` one or more interfaces and be used through them.
- `self`/`static` type hints in enum methods resolve to the enum.

Enum constants are readable both inside the enum (`self::CONST`) and from outside it (`EnumName::CONST`). Enum method bodies are type-checked like class method bodies, so a mismatched return type or an undefined variable inside an enum method is reported.

Current limitations: using a trait inside an enum is not supported.

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
- `__construct(...)` — runs at instantiation
- `__destruct()` — runs when the object is released (see below)
- `__toString()` — string coercion
- `__get($name)` — reading an undeclared property
- `__set($name, $value)` — writing an undeclared property
- `__isset($name)` — `isset()`/`empty()` on an undeclared property
- `__unset($name)` — `unset()` of an undeclared property
- `__invoke(...$args)` — calling an object directly
- `__call($name, $args)` — intercepting missing instance methods
- `__callStatic($name, $args)` — intercepting missing static methods

## Property interception (`__get`, `__set`, `__isset`, `__unset`)

When code reads, writes, tests, or removes a property that the class does not
declare, the matching magic method is invoked. This lets a class expose virtual
properties backed by any internal representation:

```php
<?php
class Config
{
    private bool $debugEnabled = true;

    public function __get(string $name): bool
    {
        return $name === "debug" && $this->debugEnabled;
    }

    public function __isset(string $name): bool
    {
        return $name === "debug" && $this->debugEnabled;
    }

    public function __unset(string $name): void
    {
        if ($name === "debug") {
            $this->debugEnabled = false;
        }
    }
}

$config = new Config();
echo isset($config->debug) ? "on" : "off";  // on  → __isset
unset($config->debug);                        //     → __unset
echo isset($config->debug) ? "on" : "off";  // off → __isset
```

`isset($obj->prop)` returns the boolean result of `__isset`; `unset($obj->prop)`
runs `__unset` for its side effects. Both fire only for properties the class does
not declare — accessing a declared property uses it directly.

Contract: `__isset` and `__unset` must be non-static and public, and each takes
exactly one argument (the property name). `__isset` returns `bool`.

## Static call interception (`__callStatic`)

`__callStatic` is the static counterpart of `__call`: a static call to a method
the class does not define is forwarded to `__callStatic`, which receives the
called method name and an array of the arguments. Subclasses inherit it.

```php
<?php
abstract class Query
{
    public static function __callStatic(string $method, array $args): string
    {
        return $method . "(" . implode(", ", $args) . ")";
    }
}

class User extends Query {}

echo User::where("active", "1");  // where(active, 1)  → __callStatic
echo User::orderBy("name");        // orderBy(name)      → __callStatic
```

Contract: `__callStatic` must be declared `public static` and takes exactly two
arguments — the method name (`string`) and the argument list (`array`).

## Destructors (`__destruct`)

A class may declare `public function __destruct(): void` to run cleanup when an
object is released. elephc uses reference counting, so the destructor runs as
soon as the last reference goes away — there is no separate garbage-collection
delay. It fires when:

- a local variable holding the object goes out of scope (including on early
  `return` and during exception unwinding);
- the variable is reassigned or `unset()`, releasing the previous object;
- the object was the last element keeping a containing array or object alive;
- the program ends, for objects still referenced by top-level variables.

The destructor runs **before** the object's properties are released, so it can
still read `$this` and its properties. This makes RAII-style cleanup natural:
acquire a resource in `__construct`, release it in `__destruct`.

```php
<?php
class TempFile
{
    private string $path;
    public function __construct(string $path)
    {
        $this->path = $path;
        file_put_contents($this->path, "scratch");
    }
    public function __destruct()
    {
        unlink($this->path);   // runs automatically when the object is released
    }
}
```

Rules and notes:

- `__destruct` must be non-static and take no arguments. Any visibility is
  allowed (PHP calls it regardless), so it may be `public`, `protected`, or
  `private`.
- A subclass without its own `__destruct` inherits its parent's.
- Taking a temporary copy of `$this` inside the destructor is safe; the object
  is not freed twice.
- Objects that form a reference cycle (e.g. `$a->next = $b; $b->next = $a;`) are
  reclaimed by the targeted cycle collector, and their destructors still run. The
  collector is eager — it reclaims an unreachable island as soon as the last
  outside reference drops — so for cyclic objects the destructor *timing* can
  differ from PHP's deferred cycle collector, and the order among the cycle's
  members is unspecified.
- **Object resurrection is not supported**: storing `$this` somewhere that
  outlives the destructor (so the object would survive) does not keep it alive —
  the object is still freed once the destructor returns. Avoid retaining `$this`
  past the end of `__destruct`.

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

User-defined attributes (e.g. `#[Author]`, `#[Pure]`, `#[Memoized]`) parse and persist in the AST. They have no compile-time semantics, but their **names** and **literal arguments** (positional and named) are reachable at runtime through lightweight helper builtins and the supported Reflection API:

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

Functions and their parameters can also be reflected. `ReflectionFunction` reads
a named function's signature, and `getParameters()` returns one
`ReflectionParameter` per declared parameter, in order:

```php
<?php
class Mailer {}

function send(string $to, Mailer $mailer, int $retries = 3, ?string $subject = null): void {}

$fn = new ReflectionFunction('send');
echo $fn->getNumberOfParameters();         // 4
echo $fn->getNumberOfRequiredParameters(); // 2

foreach ($fn->getParameters() as $param) {
    echo $param->getName();                // to, mailer, retries, subject
    echo $param->getPosition();            // 0, 1, 2, 3
    echo $param->isOptional() ? "?" : "!"; // retries and subject are optional

    if ($param->hasType()) {
        $type = $param->getType();         // ReflectionNamedType
        echo $type->getName();             // string, Mailer, int, string
        echo $type->isBuiltin() ? "b" : "c";
        echo $type->allowsNull() ? "n" : "-"; // subject (?string) allows null
    }
}
```

A parameter with no type hint reports `hasType()` as `false`, and `getType()`
returns `null`. A nullable hint such as `?string` reports `getName()` as
`string` with `allowsNull()` true. Class-typed parameters report the bare class
name with `isBuiltin()` false.

| Reflection method | Returns | Description |
|---|---|---|
| `ReflectionFunction::getName()` | `string` | The reflected function's name |
| `ReflectionFunction::getShortName()` | `string` | The name without its namespace prefix |
| `ReflectionFunction::getNumberOfParameters()` | `int` | Total declared parameters |
| `ReflectionFunction::getNumberOfRequiredParameters()` | `int` | Parameters without a default and before the first optional |
| `ReflectionFunction::getParameters()` | `ReflectionParameter[]` | One object per declared parameter, in order |
| `ReflectionParameter::getName()` | `string` | The parameter name (without `$`) |
| `ReflectionParameter::getPosition()` | `int` | Zero-based parameter index |
| `ReflectionParameter::isOptional()` | `bool` | True for a parameter with a default or variadic, and any after it |
| `ReflectionParameter::isVariadic()` | `bool` | True for the `...$rest` parameter |
| `ReflectionParameter::hasType()` | `bool` | True when the parameter declares a type |
| `ReflectionParameter::getType()` | `?ReflectionNamedType` | The declared type, or `null` when untyped |
| `ReflectionNamedType::getName()` | `string` | The type name (`int`, `string`, a class name, …) |
| `ReflectionNamedType::isBuiltin()` | `bool` | True for builtin types, false for class types |
| `ReflectionNamedType::allowsNull()` | `bool` | True when the declared type is nullable |

Limitations today:
- All arguments to `class_attribute_names()`, `class_attribute_args()`, `class_get_attributes()`, and `new ReflectionClass/Method/Property(...)` must be compile-time class/member strings. `ClassName::class` is accepted for the class-name argument of `new ReflectionClass/Method/Property(...)`, and normal named-argument / static associative-spread normalization runs before the literal-string check. Dynamic class, method, property, or attribute names require a runtime name→id lookup table that is not yet implemented.
- Attribute arguments materialized by reflection today include: string, int, float, bool, null, negation (`-N`) of a numeric literal, arrays (positional and associative, nested, with heterogeneous element types), **named arguments**, and **symbolic references** — a global constant (`#[A(SOME_CONST)]`), a class/interface constant (`#[A(C::BAR)]`), or an enum case (`#[A(E::Case)]`). `ReflectionClass`/`ReflectionMethod`/`ReflectionProperty::getAttributes()` → `getArguments()` returns named arguments under their string keys and positional arguments under their integer keys, matching PHP. A constant reference resolves to its value and an enum-case reference resolves to the case object (so reading its `->value`/`->name` works just like PHP), and `newInstance()` constructs the attribute with those arguments.
- A symbolic reference that elephc cannot resolve — for example a built-in class constant such as `Attribute::TARGET_CLASS`, which is not registered — is treated as unsupported metadata: the attribute still parses and compiles and `class_attribute_names()` still lists it, but its arguments are not reflectable through `getAttributes()`/`class_get_attributes()`/`class_attribute_args()`.
- The flat `class_attribute_args()` helper returns a positional array of scalars only; it rejects attributes whose arguments are keyed (named arguments or associative arrays, at any depth) or contain a symbolic reference. Use `ReflectionClass::getAttributes()->getArguments()` for those.
- When several attributes share a name on the same class, `class_attribute_args()` returns the args of the first match; `class_get_attributes()` does expose every occurrence as a separate `ReflectionAttribute` in source order.
- `ReflectionClass` supports `getName()` and `getAttributes()`. `ReflectionMethod` and `ReflectionProperty` currently support `getAttributes()` only; broader APIs such as `getProperties()`, `getMethods()`, and object construction through `ReflectionClass::newInstance()` are not yet available.
- `ReflectionFunction`/`ReflectionParameter` reflect named functions only (the constructor argument must be a compile-time function-name string). `ReflectionParameter::getType()` resolves a single named type (including a nullable `?T`); union and intersection parameter types, default-value reflection (`getDefaultValue()`), and per-parameter attribute reflection are not yet available. An explicit `mixed` hint is reported as untyped.

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
- Backed property hooks may read and write their own backing slot, but the short `set => expr;` form is not supported; use a block `set { ... }`.
- Shadowing a private parent property with a same-named child property is not yet supported (PHP gives them separate slots; elephc uses one slot per name)
- Class constants must be literal-or-foldable expressions; cyclic constant references are not supported.
- Class attribute names and supported literal args are exposed at runtime through `class_attribute_names()`, `class_attribute_args()`, `class_get_attributes()`, and the supported `ReflectionClass`/`ReflectionMethod`/`ReflectionProperty::getAttributes()` APIs. Function and parameter signatures are exposed through `ReflectionFunction` and `ReflectionParameter` (including `getType()`); per-parameter attribute reflection is not yet available. `#[\Override]`, `#[\Deprecated]`, and `#[\AllowDynamicProperties]` are enforced/diagnosed/honored at compile time and runtime; `#[\SensitiveParameter]` is parsed but not yet propagated to parameters (refactor of param representation and stack-trace infrastructure pending).
