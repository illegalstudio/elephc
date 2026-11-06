<?php

// `array<T>` pins an array's element type instead of leaving the compiler to infer it.
//
// Without the annotation, two call sites that disagree collapse the parameter to
// `array<mixed>` — every element becomes a heap-allocated tagged cell, for both callers.
// With it, each function keeps register-width storage for its own element type.

/**
 * Sums an array of integers.
 *
 * The elements stay raw 64-bit values: no boxing, no allocation per element.
 */
function sumInts(array<int> $numbers): int
{
    $total = 0;
    foreach ($numbers as $n) {
        $total += $n;
    }
    return $total;
}

/**
 * Joins an array of strings.
 *
 * `string` is two registers wide where `int` is one, so this is a genuinely different
 * storage layout — and it coexists with `sumInts` in the same program.
 */
function joinWords(array<string> $words, string $glue): string
{
    $out = '';
    foreach ($words as $i => $word) {
        $out .= $i === 0 ? $word : $glue . $word;
    }
    return $out;
}

/**
 * Type arguments work in return position too.
 */
function firstSquares(): array<int>
{
    return [1, 4, 9, 16];
}

/**
 * Two type arguments select the associative form: hash storage, both halves typed.
 *
 * PHP array keys are only ever integers or strings, so the key type must be `int`,
 * `string` or `mixed` — `array<float, int>` is rejected at the annotation.
 */
function totalOf(array<string, int> $amounts): int
{
    $total = 0;
    foreach ($amounts as $amount) {
        $total += $amount;
    }
    return $total;
}

// Object element types are fine: they share one representation class, the heap pointer.
class Point
{
    public function __construct(
        public int $x,
        public int $y,
    ) {}
}

function furthestFromOrigin(array<Point> $points): int
{
    $best = 0;
    foreach ($points as $p) {
        $d = $p->x * $p->x + $p->y * $p->y;
        if ($d > $best) {
            $best = $d;
        }
    }
    return $best;
}

/**
 * A generic function: the call sites bind `T`, and each distinct binding becomes its own
 * monomorphic function. `identity(5)` compiles to `I64 -> I64`, `identity('hi')` to
 * `Str -> Str`. An untyped function shared between those two call sites would widen to
 * `mixed` and box every value, for both callers.
 */
function identity<T>(T $value): T
{
    return $value;
}

/**
 * Inference reaches through an `array<T>` parameter, so the element type is what binds `T`.
 */
function firstOf<T>(array<T> $items): T
{
    return $items[0];
}

/**
 * A bound states the contract the body relies on. `idOf(5)` is rejected at the call, not
 * inside the instantiated body.
 */
class Entity
{
    public function __construct(public int $id) {}
}

function idOf<T : Entity>(T $entity): int
{
    return $entity->id;
}

/**
 * A generic CLASS needs no inference: every mention writes its type arguments.
 *
 * `Box<int>` and `Box<string>` are two ordinary classes with two storage layouts — the
 * first holds a machine integer, the second a string. Neither holds a boxed value.
 */
class Box<T>
{
    public function __construct(private T $value) {}

    public function get(): T
    {
        return $this->value;
    }
}

/**
 * A bound on a class parameter is the same contract, checked the same way: by asking the
 * class hierarchy, so a subclass satisfies it and an unrelated look-alike does not.
 */
interface Identified
{
    public function id(): int;
}

class User implements Identified
{
    public function __construct(private int $uid) {}

    public function id(): int
    {
        return $this->uid;
    }
}

/**
 * The declaration the RFC's syntax was written for: an interface parameterized by what it
 * stores, implemented at one concrete type.
 */
interface Repository<T : Identified>
{
    public function find(int $id): T;
}

class UserRepository implements Repository<User>
{
    public function find(int $id): User
    {
        return new User($id);
    }
}

/**
 * A static factory on a generic class. `Box<int>::of(1)` is told apart from the comparison
 * chain `Box < int > ::of` by the `::`, which cannot follow a comparison.
 */
class Crate<T>
{
    public const LABEL = 'crate';

    private function __construct(private T $value) {}

    public static function of(T $value): Crate<T>
    {
        return new Crate<T>($value);
    }

    public function get(): T
    {
        return $this->value;
    }
}

/**
 * `Box<Box<int>>` ends in ONE token — PHP's `>>` — and the parser is what splits it back
 * into two closes. Inference still recovers `T`: the instantiated class NAME carries its
 * arguments, so reading the name back is the inverse of writing it.
 */
function unwrap<T>(Box<Box<T>> $nested): T
{
    return $nested->get()->get();
}

echo identity(5), '|', identity('hi'), '|', identity(2.5), "\n";
echo firstOf([10, 20]), '|', firstOf(['a', 'b']), "\n";
echo idOf(new Entity(42)), "\n";
echo sumInts([1, 2, 3, 4, 5]), "\n";
echo joinWords(['compile', 'php', 'natively'], ' '), "\n";
echo sumInts(firstSquares()), "\n";
echo totalOf(['rent' => 1200, 'food' => 300, 'transit' => 90]), "\n";
echo furthestFromOrigin([new Point(1, 2), new Point(6, 3), new Point(0, 4)]), "\n";
echo (new Box<int>(41))->get() + 1, '|', (new Box<string>('boxed'))->get(), "\n";
echo (new UserRepository())->find(7)->id(), "\n";
echo unwrap(new Box<Box<int>>(new Box<int>(5))), "\n";
echo Crate<int>::of(4)->get(), '|', Crate<string>::LABEL, '|', Crate<int>::class, "\n";
echo (Crate<int>::of(1) instanceof Crate<int>) ? 'yes' : 'no', '|',
     (Crate<string>::of('s') instanceof Crate<int>) ? 'yes' : 'no', "\n";

// The type arguments need not be written when the constructor already says what it takes.
echo (new Box(7))->get(), '|', (new Box('inferred'))->get(), "\n";

// A declared element type is a contract, in both directions.
//
// An element write that would widen it is a compile error, not a silent conversion:
//
//     array<int> $pinned = [1, 2, 3];
//     $pinned[0] = 'not an int';
//
//     error: cannot store string into $pinned declared as array<int>
//
// And a body whose array does not have the declared element storage is rejected too,
// rather than handing the caller boxed pointers to read as integers:
//
//     function squares(int $n): array<int> {
//         $out = [];
//         for ($i = 1; $i <= $n; $i++) { $out[] = $i * $i; }
//         return $out;
//     }
//
//     error: declares array<int> but returns array<mixed>; the element storage differs
//
// That second one is not about the empty literal — appending a constant, a parameter or a
// `foreach` value all satisfy `array<int>`. It is about the COUNTER: PHP promotes an integer
// at the overflow boundary, so `$i` after `$i++` is `int|float`, and that is boxed storage
// rather than a packed int vector. Cast at the append to pin the element type:
//
//     for ($i = 1; $i <= $n; $i++) { $out[] = (int) ($i * $i); }

// `array<T>` is an elephc extension with no PHP equivalent, so `--strict-php` rejects it.
// A bare `array` hint stays valid PHP and is untouched.
