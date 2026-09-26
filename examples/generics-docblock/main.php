<?php

// The same generics, written as PHPStan annotations.
//
// Every line of this file is valid PHP: the type parameters live in comments. Run it with
// `php main.php` and it works; compile it with elephc and it monomorphizes, producing one
// class per type argument exactly as `class Box<T>` does in the native syntax.
//
// `examples/generics/` shows the native surface, which is more direct and cannot run on
// php-src. This one trades that directness for portability.

/**
 * A container that keeps whatever it was given, at that type.
 *
 * `@template` declares the parameter, `@var` types the property, and `@param`/`@return` type
 * the members. Only an annotation MENTIONING `T` is read as a declaration — `@param int $n`
 * in this class would stay the ordinary PHPStan annotation it is anywhere else.
 *
 * @template T
 */
class Box
{
    /** @var T */
    private $value;

    /** @param T $value */
    public function __construct($value)
    {
        $this->value = $value;
    }

    /** @return T */
    public function get()
    {
        return $this->value;
    }

    /** @param T $value */
    public function set($value): void
    {
        $this->value = $value;
    }

    // No type parameter is mentioned, so this is left exactly as written.
    public function describe(string $prefix): string
    {
        return $prefix;
    }
}

/**
 * A promoted constructor parameter is one declaration in the source and two in the AST — the
 * parameter and the property. Both are retyped, or the class would disagree with itself.
 *
 * @template T
 */
final class Pair
{
    /**
     * @param T $first
     * @param T $second
     */
    public function __construct(private $first, private $second) {}

    /** @return T */
    public function first()
    {
        return $this->first;
    }

    /** @return T */
    public function second()
    {
        return $this->second;
    }
}

/**
 * An interface is a template too, and `@extends` is how one names the instantiation it
 * inherits — the interface's `extends` list is where elephc records those arguments.
 *
 * @template T
 */
interface Reader
{
    /** @return T */
    public function read();
}

/**
 * `@implements Reader<T>` passes this class's own parameter through to the interface.
 *
 * @template T
 * @implements Reader<T>
 */
abstract class Holder implements Reader
{
    /** @param T $slot */
    public function __construct(protected $slot) {}

    /** @return T */
    public function read()
    {
        return $this->slot;
    }
}

/**
 * A class with no type parameters of its own still names what it inherits. Naming someone
 * else's instantiation is not the same as declaring a parameter.
 *
 * @extends Holder<int>
 * @implements Reader<int>
 */
final class IntHolder extends Holder
{
    public function doubled(): int
    {
        return $this->read() * 2;
    }
}

/**
 * The function surface, for comparison: it reads the same annotations into the same fields.
 *
 * @template T
 * @param array<T> $items
 * @return T
 */
function firstOf(array $items)
{
    return $items[0];
}

$number = new Box(41);
$number->set($number->get() + 1);
$word = new Box('hi');

echo $number->get(), '|', $word->get(), '|', $word->describe('ok'), "\n";

$ints = new Pair(3, 4);
$words = new Pair('a', 'b');

echo $ints->first() + $ints->second(), '|', $words->first() . $words->second(), "\n";

$held = new IntHolder(21);

echo $held->doubled(), '|', $held->read(), "\n";
echo firstOf([10, 20]), '|', firstOf(['x', 'y']), "\n";

// One divergence is worth knowing about, and it is monomorphization rather than this surface:
// the template does not exist at runtime. Under php-src `Box` is an ordinary class; compiled by
// elephc only `Box<int>` and `Box<string>` exist, and `class_exists('Box')` is false. Native
// `class Box<T>` behaves the same way.
echo get_class($number), '|', get_class($word), '|', get_class($ints), "\n";
