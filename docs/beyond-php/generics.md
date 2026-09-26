---
title: "Generics"
description: "Generic functions, classes and methods that compile to one specialized version per type, instead of erasing to boxed mixed."
sidebar:
  order: 10
---

Generic functions, classes and methods, compiled rather than erased: each type you use produces
its own specialized version, so a container costs what writing it out by hand costs.

> **Strict mode:** the native syntax is an elephc extension with no PHP equivalent, and
> [`--strict-php`](../compiling/cli-reference.md#strict-php-mode) rejects it. The PHPStan
> docblock form below is ordinary PHP and is accepted. A bare `array` hint is untouched either
> way.

## Quick start

Four things you can write. Each one compiles to code with no boxing — see
[Why it matters](#why-it-matters) for the numbers.

**A generic function.** The type arguments are inferred from the call:

```php
<?php
function firstOf<T>(array<T> $items): T { return $items[0]; }

echo firstOf([1, 2, 3]);      // firstOf<int>
echo firstOf(["a", "b"]);     // firstOf<string>
```

**A generic class.** Write the arguments, or let `new` infer them:

```php
<?php
class Box<T>
{
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
}

$a = new Box<int>(41);   // written
$b = new Box("hi");      // inferred: Box<string>
```

**A generic method**, with its own parameter, bound by the call rather than the class:

```php
<?php
class Box<T>
{
    public function __construct(private T $value) {}

    public function map<U>(callable(T): U $f): Box<U>
    {
        return new Box<U>($f($this->value));
    }
}

$lengths = (new Box("hello"))->map(fn(string $s): int => strlen($s));   // Box<int>
```

**The same thing in plain PHP.** Annotations are comments, so the file still runs on php-src
and still passes `--strict-php` — see [Writing it in plain PHP](#writing-it-in-plain-php):

```php
<?php
/** @template T */
class Box
{
    /** @var T */
    private $value;

    /** @param T $value */
    public function __construct($value) { $this->value = $value; }

    /** @return T */
    public function get() { return $this->value; }
}
```

Two rules are worth knowing before you start:

- An instantiation is a real class. `Box<int>` and `Box<string>` are two distinct classes, and
  the template itself does not exist at runtime — `class_exists('Box')` is `false`.
- Everything here is an elephc extension except the docblock form. `--strict-php` rejects the
  native syntax and accepts the annotations.

## Why it matters

Elephc already specializes an untyped `array` parameter to the element type it sees at the
call site. That specialization has arity one: a second call site with a different element
type widens the parameter to `array<mixed>`, and every element becomes a pointer to a
heap-tagged cell — for both callers, not just the second.

```php
<?php
function firstOf(array $a) { return $a[0]; }

echo firstOf([1, 2, 3]);        // alone: a: array<int>  -> I64
echo firstOf(["a", "b", "c"]);  // both:  a: array<mixed> -> Heap(Mixed)
```

Adding the second call retroactively deoptimizes the first. A declared element type stops
that: each function keeps the storage its own annotation asks for.

```php
<?php
function firstInt(array<int> $a): int { return $a[0]; }
function firstStr(array<string> $a): string { return $a[0]; }
```

The two coexist in one program with no boxing. `int` is one register, `string` is two, and
objects, arrays and `mixed` share the heap-pointer representation.

A generic class reaches the same floor. Same shape, 3,000,000 iterations:

| Container | Allocations | Time |
| --- | --- | --- |
| `class Box<T>` used as `Box<int>` | 6,000,001 | 0.11 s |
| hand-written `private int $value` | 6,000,001 | 0.11 s |
| erased `private mixed $value` | 12,000,001 | 0.21 s |

An instantiation costs what writing the class out by hand costs, and half what erasing it does.

## Typed arrays

`array<T>` pins an array's element type, and `array<K, V>` its key and value types. It is the
narrowest of the generic forms — no declaration to write, just a sharper hint — and the one most
existing code can adopt unchanged.

### Where it is accepted

Every type position: parameters, return types, properties, class constants, typed locals,
closure and arrow signatures.

```php
<?php
function sumInts(array<int> $numbers): int { /* ... */ }
function makeIds(): array<string> { return ["a", "b"]; }

array<int> $pinned = [1, 2, 3];
```

Element types may be unions or nullable: `array<int|string>`, `array<?int>`.

Element types nest: `array<array<int>>`, `array<array<array<int>>>` and
`array<string, array<int>>` all parse. The trailing `>>` is PHP's right-shift token and the
lexer cannot tell it from two closes, so the type parser splits it — the same rule that makes
`Box<Box<int>>` work, described under "`>>` is one token".

### Associative arrays

Two type arguments select the associative form, which has hash storage rather than a packed
element vector:

```php
<?php
function lookup(array<string, int> $ages, string $name): int
{
    return $ages[$name];
}
```

Both halves stay typed. The lowered signature is `Heap(Hash) php=array<string, int>` returning
a raw `I64` — no boxing on either side.

PHP array keys are only ever integers or strings, so the key type must be `int`, `string` or
`mixed`. Anything else is rejected at the annotation:

```php
<?php
function f(array<float, int> $m): int { return 0; }
// error: array<K, V> key type must be int, string, or mixed, got float
```

The indexed and associative forms are not interchangeable, since one is a packed vector and
the other a hash table. A body that returns the wrong one is rejected:

```php
<?php
function f(): array<string, int> { return [1, 2, 3]; }
// error: declares array<string, int> but returns array<int>; the storage differs
```

An empty literal carries no keys and no element storage, so it satisfies either form:

```php
<?php
function f(): array<string, int> { return []; }   // ok
```

### The declaration is a contract

A declared element type is enforced in both directions. This is the difference between an
annotation and a comment.

An element write that would widen the declared type is a compile error, not the silent
conversion an inferred array gets:

```php
<?php
array<int> $pinned = [1, 2, 3];
$pinned[0] = "not an int";
// error: cannot store string into $pinned declared as array<int>

array<string, int> $ages = ["alice" => 30];
$ages["bob"] = "not an int";
// error: cannot store string into $ages declared as array<string, int>
```

A `mixed` element or value type absorbs every write, so `array<mixed>` and
`array<string, mixed>` impose no contract and stay freely writable.

An argument whose element type disagrees is rejected rather than widening the parameter:

```php
<?php
function firstOf(array<int> $a): int { return $a[0]; }
firstOf(["a", "b"]);
// error: Function 'firstOf' parameter $a expects array<int>, got array<string>
```

And a body whose array does not have the declared element storage is rejected, rather than
handing the caller boxed pointers to read as integers:

```php
<?php
function squares(int $n): array<int> {
    $out = [];
    for ($i = 1; $i <= $n; $i++) { $out[] = $i * $i; }
    return $out;
}
// error: declares array<int> but returns array<mixed>; the element storage differs
```

That last check is not redundant with ordinary assignability. `mixed` is compatible with
every type, so an `array<mixed>` body would otherwise satisfy a declared `array<int>` — and
the element layout lives in the array header tag, not in the static type, so every caller
would read the callee's boxed pointers as integers.

> Building an array by appending into an empty literal infers `array<mixed>`, so it cannot
> satisfy a concrete declared element type yet. Return a literal, or take the array as a typed
> parameter — see [Limitations](#limitations).

A bare `array` return imposes no element contract and keeps accepting the append idiom.

## Generic functions

A function may declare type parameters, which its call sites bind:

```php
<?php
function identity<T>(T $value): T { return $value; }
function firstOf<T>(array<T> $xs): T { return $xs[0]; }
function pairUp<K, V>(K $key, V $value): string { return $key . '=' . $value; }
```

> **Strict mode:** generic functions are an elephc extension. `--strict-php` rejects them.

### Each call site gets its own function

A generic declaration is a **template**, not a function. It is never compiled as written.
Every call infers its type arguments and selects a monomorphic instantiation, which is an
ordinary function from that point on:

```php
<?php
echo identity(5);       // identity<int>:    I64 -> I64
echo identity('hi');    // identity<string>: Str -> Str
echo identity(2.5);     // identity<float>:  F64 -> F64
```

Three functions, none of them boxed. An untyped function shared between those three call sites
would widen to `mixed` and pay a heap allocation per value, for all three callers.

A template that is never called emits no code at all.

### Inference

Type arguments come from the argument types, matched against the declared parameter types:

| Declaration | Argument | Binding |
| --- | --- | --- |
| `T $x` | `int` | `T = int` |
| `array<T> $xs` | `array<string>` | `T = string` |
| `array<K, V> $m` | `array<string, int>` | `K = string`, `V = int` |
| `?T $x` | `int` | `T = int` |

Anything a call site does not constrain is an error rather than a silent `mixed`:

```php
<?php
function f<T>(int $x): int { return $x; }
f(1);
// error: does not determine type parameter <T>

function pair<T>(T $a, T $b): T { return $a; }
pair(1, 'two');
// error: binds type parameter <T> to both int and string
```

A template may call another template; instantiating the outer one instantiates the inner one
in turn.

A type parameter is substituted at **every** type position, not only in the signature — a
typed local, a `buffer<T>` element type, a nested closure's parameters and return type:

```php
<?php
function via<T>(T $value): T {
    T $held = $value;
    $inner = function (T $x): T { return $x; };
    return $inner($held);
}
```

### Writing the type arguments at the call

Inference is the default and covers the ordinary case. Where it cannot reach — or where you want
a different answer than the arguments would give — write the arguments at the call:

```php
<?php
function emptyList<T>(): array<T> { return []; }

$ints = emptyList<int>();          // nothing to infer from: only the call can say
$ints[] = 7;

function identity<T>(T $v): T { return $v; }
var_dump(identity<float>(1));      // float(1), not int(1)
```

The list is positional and may stop early only where the remaining parameters declare defaults:

```php
<?php
function pair<A, B = string>(A $a, B $b): B { return $b; }
echo pair<int>(1, 'x');            // B falls back to its default

function both<A, B>(A $a, B $b): string { return 'ok'; }
both<int>(1, 'x');
// error: writes 1 type argument but leaves <B> unbound, and it declares no default
```

Bounds are checked against a written argument exactly as against an inferred one, and the same
syntax works on a generic method:

```php
<?php
$b = new Box<int>(21);
echo $b->map<string>(fn (int $x): string => 'n' . $x * 2)->get();   // n42
```

An ordinary comparison is untouched: `identity<int>(41)` is read as type arguments only because
the `(` follows the list, and php-src rejects the comparison reading of the same tokens outright —
`<` is non-associative in PHP 8.

### Polymorphic recursion is rejected

A generic function that calls itself at a type built from its own type parameter has no finite
set of instantiations, and is rejected by naming the type that ran away:

```php
<?php
function deep<T>(T $value): int { return deep([$value]); }
// error: Generic function 'deep' instantiates itself at an ever-deeper type:
//        <T> reached array<array<array<array<array<array<array<array<int>>>>>>>>
```

### Bounds and defaults

A type parameter may declare an upper bound, and a default for when nothing constrains it:

```php
<?php
function idOf<T : Entity>(T $entity): int { return $entity->id; }
function labelFor<K = string>(int $n): int { return $n; }
```

A bound states the contract the body relies on, so a violating call is rejected at the **call**
rather than inside the instantiated body:

```php
<?php
idOf(5);
// error: binds type parameter <T> to int, which does not satisfy its bound Entity
```

A default replaces the unconstrained-parameter error. Inference still wins over it when an
argument position determines the parameter.

A bound is satisfied by the bound type itself, by any subclass, and by any class implementing
it when the bound names an interface:

```php
<?php
interface Identifiable { public function id(): int; }
class Entity implements Identifiable { public function id(): int { return 1; } }
class User extends Entity {}
class Unrelated { public function id(): int { return 0; } }

function idOf<T : Entity>(T $e): int { return $e->id(); }
function anyId<T : Identifiable>(T $e): int { return $e->id(); }

idOf(new User());        // ok — subclass
anyId(new Entity());     // ok — implements the interface
idOf(new Unrelated());   // error, despite declaring the same method
```

Bound checking asks the class hierarchy, not the spelling, so a class that merely resembles the
bound does not satisfy it.

## Generic classes and interfaces

The syntax the RFC proposed, on the declaration a program actually reaches for:

```php
<?php

interface Entity {
    public function id(): int;
}

interface Repository<T: Entity> {
    public function find(int $id): T;
}

class UserRepository implements Repository<User> {
    public function find(int $id): User { return new User($id); }
}
```

A generic class is instantiated the same way a generic function is — one ordinary class per
distinct type argument list — but it needs **no inference** to get there. Every mention writes
its arguments:

```php
class Box<T> {
    public function __construct(private T $v) {}
    public function get(): T { return $this->v; }
}

$i = new Box<int>(41);       // Box<int>:    property I64
$s = new Box<string>("ok");  // Box<string>: property Str
```

Two classes, two storage layouts, no boxing. `Box<int>`'s property is a machine integer, not a
tagged value that happens to hold one.

Because the arguments are written rather than inferred, instantiation is **pure syntax** and
runs before the type checker — the checker never learns that generic classes exist. It answers
exactly one question about them: whether a type argument satisfies its parameter's bound, which
is a subtyping question and needs the class table.

### Where the type arguments can appear

```php
new Box<int>(1);                       // construction
Box<int> $b = ...;                     // a declared local
function f(Box<int> $b): Box<string>   // parameter and return
class Small extends Box<int> {}        // an inherited parent
class R implements Repository<User> {} // an inherited interface
private Box<int> $field;               // a property
Box<int>::of(1);                       // a static method
Box<int>::LABEL;  Box<int>::class;     // a class constant, and the name itself
$x instanceof Box<int>;                // an instanceof target
catch (Box<int> $e)                    // a caught class
Box<Box<int>>                          // nested — see below
Box<array<int>>                        // any type, not only classes
```

`Box<int>::class` evaluates to `"Box<int>"` — the instantiated class's real name, which is what
`get_class()` on one of its instances returns too. `Box<int>` and `Box<string>` are unrelated
classes, so `$stringBox instanceof Box<int>` is `false`.

Telling `Box<int>::of(1)` apart from the comparison chain `Box < int > ::of` is what the `::`
does: it cannot follow a comparison, so a balanced argument list followed by `::` is
unambiguous. `A < $b && $b > $c` still parses as two comparisons, because no `::` follows.

`instanceof` is the one mention with no such token after it, so there the list is read as type
arguments only when what follows it cannot begin an expression — `instanceof Box<int>;` and
`instanceof Box<int> && $y` are claimed, `instanceof Box < A > $y` is left as comparisons. That
costs nothing: php-src rejects `$x instanceof Foo < BAR > $z` outright, because `<` is
non-associative in PHP 8.

### `>>` is one token

`Box<Box<int>>` ends in a single `>>` — PHP's right-shift operator, which the lexer cannot tell
from a type close because `$a >> $b` is the same two characters. The parser splits it: the
innermost list consumes the token and leaves a credit the enclosing list spends. Three levels
lex as `>>` then `>`, four as `>>` then `>>`, and both fall out of the same rule.

### Bounds and defaults on a class

Identical to the function form, and checked by the same predicate:

```php
class Pen<T: Animal> { ... }
new Pen<Dog>(new Dog());   // ok — a subclass satisfies the bound
new Pen<int>(3);           // error: binds <T> to int, which does not satisfy its bound Animal

class Pair<K, V = string> { ... }
new Pair<int>(1, "x");     // V defaults to string
```

### A bare template name

A template mentioned with no arguments is read as the instantiation its own declaration
describes — each parameter at its bound, or its default, or `mixed` when it has neither — and the
compiler says so:

```php
<?php
class Repo<T: Entity> { ... }

function idOf(Repo $r): int { return $r->item()->id(); }
// warning: 'Repo' names a template and is read as 'Repo<Entity>'; each instantiation is its own
//          class, so this accepts another instantiation only where the parameter is declared
//          covariant. Write the type arguments to say which one you mean
```

The warning is the point. `Repo<Entity>` and `Repo<User>` are two classes, so the bare form takes
exactly one of them — unless the parameter is declared covariant, which is what makes the reading
useful:

```php
<?php
class Repo<out T: Entity> { ... }

function idOf(Repo $r): int { return $r->item()->id(); }
echo idOf(new Repo<User>(new User()));   // ok: `out T` admits the subtype instantiation
```

With no bound, the reading is `mixed` — erasure, and boxed storage. Write the argument instead.

### Generic functions over generic classes

```php
function unwrap<T>(Box<Box<T>> $b): T { return $b->get()->get(); }

echo unwrap(new Box<Box<int>>(new Box<int>(5)));   // 5
```

Inference recovers `T` here even though the argument's class is, by that point, an ordinary
class called `Box<Box<int>>`: the instantiated **name** encodes its arguments losslessly, and
reading it back through the language's own type grammar is the inverse of writing it.

### The type arguments can be inferred at `new`

Writing them is always allowed, but a constructor that already says what it takes determines
them on its own:

```php
class Box<T> {
    public function __construct(private T $v) {}
    public function get(): T { return $this->v; }
}

$i = new Box(41);      // Box<int>
$s = new Box("ok");    // Box<string>
```

Same inference a generic function call uses, over the constructor's parameters instead of the
function's — one rule, so the two cannot drift.

A named static factory infers the same way, which matters because PHP allows exactly one
`__construct` — a factory is how a codebase offers a second way to build something:

```php
class Box<T> {
    public static function of(T $v): Box<T> { return new Box<T>($v); }
}

Box::of(7);      // Box<int>
Box::of("x");    // Box<string>
```

Four things it will not do quietly:

```php
class Holder<T> { public function __construct() {} }
new Holder();          // error: no constructor parameter mentions <T>, so write it

new Box(anything());   // warning: infers <T> as mixed, so this instantiation is boxed
new Box<mixed>(...);   // no warning — saying it on purpose is not the same mistake

new Box(null);         // error: null says nothing about what the container holds
```

`null` is the one worth explaining. Inference binds `T` to the null type and substitutes it,
which used to produce `private null $v` and a message about a type in a declaration nobody
wrote. `new Box<?int>(null)` says what was meant.

One `new` inside a generic function is reached once per instantiation of it, so a single
position legitimately means two classes — and it resolves to each:

```php
function wrap<T>(T $v): Box<T> { return new Box($v); }
wrap(1);      // builds a Box<int>
wrap("a");    // builds a Box<string>
```

### `self` yes, `static` and `parent` no

`Box<self>` becomes `Box<A>` inside class `A`: `self` is lexical, so the instantiating pass
knows it. `static` is late-bound — `Box<static>` in a parent means a different class per
subclass — and `parent` names an inheritance clause that pass does not carry. Both are refused
by name rather than guessed at.

### Static state is per instantiation

`Box<int>` and `Box<string>` are two classes, so they have two of everything a class owns —
including static properties:

```php
class Counter<T> {
    public static int $made = 0;
    public function __construct(T $v) { static::$made++; }
}

new Counter<int>(1);
new Counter<int>(2);
new Counter<string>("x");

echo Counter<int>::$made;     // 2
echo Counter<string>::$made;  // 1
```

The template itself does not exist at runtime, for the same reason: `class_exists('Box')` is
`false` while `class_exists('Box<int>')` is `true`, and `$b instanceof Box` is `false`. Worth
knowing before migrating an existing hierarchy — an `instanceof Box` elsewhere in a codebase
stops matching once `Box` becomes generic.

This is where monomorphization and erasure genuinely differ. An erased implementation — PHPStan's
model, and the declined RFC's — has one `Counter` and one counter, so the same program prints 3
twice. Neither answer is wrong; they are different languages, and this one is the one where
`Counter<int>`'s property is a machine integer.

### Errors

Every way of getting it wrong is a compile error, never a silent widening:

```php
new Plain<int>();                 // 'Plain' declares no type parameters
new Pair<int, string, bool>(...); // 'Pair' takes 2 type argument(s) but 3 were given
new Pair<int>(1, "x");            // needs a type argument for <B>, which has no default
```

### Generic methods

A method may declare type parameters of its own, separate from its class's:

```php
class Pair<T>
{
    public function __construct(private T $left) {}
    public function left(): T { return $this->left; }

    public function withRight<U>(U $right): U { return $right; }
}

$p = new Pair<int>(1);
echo $p->withRight("two"), $p->withRight(3);   // two3
```

`T` is bound when the CLASS is instantiated, `U` when the METHOD is called, so one class
instantiation carries as many method instantiations as its call sites ask for. Bounds work the
same way they do on a function — `idOf<E : Entity>($e)` is checked at the call, against the class
table.

The type arguments are always INFERRED, because there is no syntax for writing them at a call
site: `$b->map<string>(…)` would have to be told apart from `$b->map < $x`, which is valid PHP.
So a type parameter has to be MENTIONED by a parameter for anything to bind it. A bare `callable`
mentions nothing, which is what typed callables are for.

## Typed callables

`callable(int): string` declares a callable's shape. A bare `callable` is untouched and means
exactly what it always did; the declared form exists so a type parameter can reach through a
callback:

```php
class Box<T>
{
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }

    public function map<U>(callable(T): U $f): Box<U>
    {
        return new Box<U>($f($this->value));
    }
}

$b = new Box<int>(21);
$s = $b->map(fn(int $n): string => "n=" . ($n * 2));   // Box<string>
$len = $s->map(fn(string $t): int => strlen($t));      // Box<int>
```

The binding comes from the closure's OWN declared types, not from its type: a closure's type is
`callable` and carries no signature, so inference reads the argument expression when — and only
when — the declared parameter is a `callable(…): …`. That has a consequence worth knowing: only a
closure LITERAL can answer. A callable held in a variable or named by a string has no declared
types at the call site, and a parameter it cannot determine is reported rather than guessed.

Storage is unchanged — one callable descriptor, exactly as a bare `callable`. The signature is
checker-side only, which is why nothing in the backend has a notion of it.

The return type is required: a signature that declares only its parameters says nothing about
what comes back, which is the half inference usually needs. PHPStan spells it the same way.

An instantiated generic method is dispatched STATICALLY and never takes a vtable slot. That is
not an optimization: two instantiations of one class must keep identical method sets, or their
slot numbering diverges and a variance widening takes the slot from one and indexes the other.

## Enums

An enum may implement a generic interface at a concrete type:

```php
interface Labelled<T> { public function label(): T; }

enum Suit: string implements Labelled<string>
{
    case Hearts = 'H';
    case Spades = 'S';

    public function label(): string { return $this->value; }
}

function show(Labelled<string> $l): string { return $l->label(); }
show(Suit::Hearts);   // H
```

An enum never declares type parameters of its own — there is no `enum Suit<T>` — so the only
generic half it carries is the arguments written on what it implements. Two enums may implement
one template at different types and get the two distinct interfaces.

## Variance

Two instantiations are unrelated by default, because monomorphization makes them two real
classes. `+T` and `-T` relate them:

```php
class Box<+T>
{
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
}

function readAnimal(Box<Animal> $box): string { return $box->get()->name(); }

readAnimal(new Box<Dog>(new Dog()));    // accepted: Dog is an Animal
```

`+T` is covariant — `Box<Dog>` may be used where `Box<Animal>` is expected. `-T` is
contravariant and goes the other way: a `Sink<Animal>` may stand in for a `Sink<Dog>`, because
anything that consumes an `Animal` consumes a `Dog`.

Three spellings reach the same declaration: the symbols `+T` / `-T`, the words `out T` / `in T`,
and PHPStan's tags `@template-covariant` / `@template-contravariant`. The words take one token of
lookahead, so `<out>` is still a type parameter NAMED `out` and only `<out T>` is a marker.

The widened value keeps its own class. `get_class()` still answers `Box<Dog>`, and the receiver
still runs `Box<Dog>`'s compiled methods — nothing is boxed, copied, or erased at the boundary.

**A marker is a promise, and the declaration has to keep it.** Under `+T` the parameter may only
be produced; under `-T` only consumed:

```php
class Box<+T> {
    public function set(T $value): void {}    // error: '+T' appears in an input position
}
```

Polarity composes, so a covariant parameter can be rejected inside a return type:
`sink(): Sink<T>` consumes `T` when `Sink` is contravariant. Three positions are worth knowing:

- A **constructor is exempt**. It cannot be reached through a widened reference — `new
  Box<Animal>(...)` names the instantiation it builds — and counting it would make `+T`
  impossible for every container that stores a `T`.
- A **writable public property** is read and written through the widened reference, so it admits
  no marker. `readonly` removes the write, and with it the reason.
- A **PHP array is a value**, so `all(): array<T>` stays legal under `+T`: a `T` read out of the
  returned array cannot be written back into the container.

**Widening requires identical storage, which is the one rule erasure never needs.** Two objects
are two pointers, so `Box<Dog>` to `Box<Animal>` is the same bytes. `int` is register-width and
`mixed` is a boxed tagged cell, so `Box<int>` to `Box<mixed>` is refused even though `int` is
assignable to `mixed` — widening it would have to materialize a copy, and a copy is a different
object. The diagnostic says so, because the bare mismatch reads as though the marker had been
ignored:

```
Function 'f' parameter $b expects Box<mixed>, got Box<int> — '+T' is covariant, but a widening
has to be the same bytes, and these two type arguments do not share storage
```

Before reaching for a marker, note that a bounded generic function usually says it better:

```php
function readAnimal<T : Animal>(Box<T> $box): string { return $box->get()->name(); }
```

That instantiates `readAnimal<Dog>`, which keeps `Dog` through the whole body. `+T` widens to
`Animal` and gives that up. Covariance exists in erased languages to recover what erasure took;
here nothing was taken.

## Writing it in plain PHP

elephc reads `@template`, `@param` and `@return` directly. An annotated file compiles to the
same monomorphic instantiations as native syntax — and stays valid PHP, so it still runs on
php-src and still passes `--strict-php`:

```php
<?php
/**
 * @template T
 * @param array<T> $items
 * @return T
 */
function firstOf(array $items)
{
    return $items[0];
}

echo firstOf([1, 2, 3]);    // firstOf<int>:    array<int> -> I64
echo firstOf(['a', 'b']);   // firstOf<string>: array<string> -> Str
```

PHPStan reads that annotation and **checks** it. elephc reads the same annotation and
**compiles** it.

`@template T of Foo` is the bound (`T : Foo` in native syntax) and `@template T = string` the
default; both reach the same declaration.

### A class

`@template` on a class makes it a template, exactly as `class Box<T>` does. `@var` types the
property, `@param`/`@return` type the members, and `@extends`/`@implements` name the
instantiation the declaration inherits — the four things native syntax writes inside the `<>`:

```php
<?php
/** @template T */
interface Reader
{
    /** @return T */
    public function read();
}

/**
 * @template T
 * @implements Reader<T>
 */
abstract class Holder implements Reader
{
    /** @param T $slot */
    public function __construct(protected $slot) {}

    /** @return T */
    public function read() { return $this->slot; }
}

/**
 * @extends Holder<int>
 * @implements Reader<int>
 */
final class IntHolder extends Holder
{
    public function doubled(): int { return $this->read() * 2; }
}
```

`@extends`/`@implements` are matched to the written `extends`/`implements` list by basename,
because the annotation uses whatever spelling is in scope and this runs before name resolution.
An inherited name left unannotated keeps no arguments, aligned index by index the way the parser
records `implements Repository<User>, Countable`.

A class with no type parameters of its own may still carry `@extends`/`@implements`: naming the
instantiation you inherit is not the same as declaring a parameter.

`examples/generics-docblock/main.php` is the whole surface in one file that `php` runs and
elephc compiles; `examples/generics/` is the native equivalent, which php-src cannot parse.

Inside a generic class, only an annotation that MENTIONS one of its type parameters is honoured.
`@param int $n` on a method of `Box<T>` stays the ordinary PHPStan annotation it is anywhere
else — `@template` on the class does not promote a whole body of comments into declarations the
compiler enforces.

A promoted constructor parameter is retyped together with the property it promotes, which is one
declaration in the source and two in the AST.

One divergence is worth stating plainly: the annotated file runs on php-src, where `Holder` is
an ordinary class. Compiled by elephc the template is stripped, so `class_exists('Holder')` is
`false` and only `Holder<int>` exists. That is monomorphization, not the docblock surface —
native `class Holder<T>` behaves the same way.

Rules that keep an existing codebase safe:

- A doc comment with no `@template` changes nothing. `@param`/`@return` alone carry no type
  parameter, and acting on them would re-type every annotated PHP file in the world.
- An annotation the language has no type for — `non-empty-list<T>` and similar PHPStan forms —
  is ignored rather than failing the compile of a file that is valid PHP.
- Written syntax wins over an annotation, so a file can migrate one declaration at a time. For a
  class the test is whether the parser built a generic half at all: `class Repo implements
  Repository<User>` has already said natively what it inherits.

Types inside an annotation go through the ordinary type grammar, so `array<string, Foo>` means
one thing in the language and in a docblock.

## Limitations

- **A type parameter nothing mentions must be written.** Inference works from the argument
  types, so a parameter no signature position names cannot be inferred —
  [write it at the call](#writing-the-type-arguments-at-the-call). A bare `callable` names
  nothing either; [typed callables](#typed-callables) are the way to reach through a callback.
- **Only a closure literal drives callback inference.** A callable held in a variable or named
  by a string carries no declared types at the call site.
- **A loop counter is `int|float`, so a list built from one is not `array<int>`.** PHP promotes
  an integer at the overflow boundary, so `$i` after `$i++` is one or the other, and that is
  boxed storage rather than a packed `int` vector. Cast at the append to pin it:

  ```php
  <?php
  function upTo(int $n): array<int> {
      $out = [];
      for ($i = 0; $i < $n; $i++) { $out[] = (int) $i; }
      return $out;
  }
  ```

  Appending a constant, a parameter, or a variable assigned before the loop needs no cast, and
  neither does `foreach`.
- Variance composes only through the forms elephc has a type for. A `T` reached through a
  builtin container keeps the enclosing polarity; a `T` inside a template with no marker is
  invariant.
- **A declaration inside a conditional is not part of the compiled program**, and a template
  there is no exception — the mention says so. This is not a generics limitation: an ordinary
  `class Foo` inside `if (!class_exists('Foo')) { … }` is just as invisible.
