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

Property type declarations are checked at compile time. Defaults and later assignments must be compatible with the declared type, including constructor assignments through untyped parameters. Nullable shorthand (`?T`) and union storage use the compiler's boxed mixed representation internally. `void` and `callable` property types are rejected.

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

Promoted properties support `public`, `protected`, `private`, `readonly`, nullable and union type declarations, and constructor parameter defaults. Variadic promotion is rejected, matching PHP. By-reference promoted properties are not supported yet.

## Instance methods and $this
Virtual dispatch for overrides.
Private methods are not virtual.

## parent::method()
Direct parent implementation call.

## self::method()
Binds to lexical class, not runtime child.

## static::method()
Late static binding — resolves against called class at runtime.

## Static methods
Called with `::`, no `$this`.

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

## Limitations
- No static or abstract properties
- No by-reference promoted properties
- No property redeclaration across inheritance chain
