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

class Product implements Labeled {
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

## Properties
- `public`, `protected`, `private` visibility
- Optional default values
- `readonly` properties (only assigned in `__construct`)
- `readonly class` makes all properties readonly

## Constructor
Called automatically with `new`:
```php
$p = new Point(3, 4);
```

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
- No `final` classes or methods
- No property type declarations
- No constructor promotion
- No property redeclaration across inheritance chain
