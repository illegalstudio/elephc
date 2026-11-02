<?php

// Generics written as PHPStan annotations: function-level and method-level `@template`.
//
// Every line of this file is valid PHP: the type parameters live in comments. Run it with
// `php main.php` and it works; compile it with elephc and each call is specialized to the
// types it is given, exactly as `function id<T>(T $v): T` does in the native syntax.
//
// Only FUNCTION and METHOD templates are read. A `@template` above a CLASS, an interface or a
// trait (and its `@extends` / `@implements`) is left alone, because php knows one class under
// its written name: `class_exists('Box')`, `new $class` and `get_class()` must keep working.
// A class that needs a real type parameter uses the native syntax (`class Box<T>`, see
// `examples/generics/`), which cannot run on php-src.

/**
 * The function surface: `@template` declares the parameter, `@param` and `@return` type the
 * signature. Only an annotation MENTIONING `T` is read as a declaration.
 *
 * @template T
 * @param array<T> $items
 * @return T
 */
function firstOf(array $items)
{
    return $items[0];
}

/**
 * A bound and a default are read too: `T of Countable`, `U = string`.
 *
 * @template T of Countable
 * @param T $items
 */
function size($items): int
{
    return count($items);
}

// A class that is NOT generic: a method can still declare its own template.
class Identity
{
    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function copy($value)
    {
        return $value;
    }

    /**
     * @template T
     * @param T ...$values
     * @return T
     */
    public static function first(...$values)
    {
        return $values[0];
    }

    // No type parameter is mentioned, so this is left exactly as written.
    public function describe(string $prefix): string
    {
        return $prefix;
    }
}

// Enum methods bind their own templates while the case keeps its singleton identity.
enum IdentityCase
{
    case One;

    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function copy($value) { return $value; }
}

echo firstOf([10, 20]), '|', firstOf(['x', 'y']), "\n";
echo size([1, 2, 3]), '|', size(new ArrayObject([1, 2])), "\n";

$identity = new Identity();
echo $identity->copy(7), '|', $identity->copy('seven'), '|', $identity->describe('ok'), "\n";
echo Identity::first(3, 4), '|', Identity::first('a', 'b'), "\n";
echo IdentityCase::One->copy(8), '|', IdentityCase::One->copy('eight'), "\n";

// The class keeps its written name: php and elephc agree, and `class_exists` finds it.
echo get_class($identity), '|', var_export(class_exists('Identity'), true), "\n";
