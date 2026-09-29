<?php

// Reflecting over a function's signature at compile time.
//
// elephc resolves reflection in a closed world: ReflectionFunction and
// ReflectionParameter read metadata baked from the function declaration, so
// parameter names, positions, optionality and declared types are all known.

class Mailer {}

function send(string $to, Mailer $mailer, int $retries = 3, ?string $subject = null): void
{
}

$fn = new ReflectionFunction('send');

echo $fn->getName(), " takes ", $fn->getNumberOfParameters(), " parameters",
    " (", $fn->getNumberOfRequiredParameters(), " required)\n";

foreach ($fn->getParameters() as $param) {
    echo "  #", $param->getPosition(), " \$", $param->getName();

    if ($param->hasType()) {
        $type = $param->getType();
        echo ": ";
        if ($type->allowsNull()) {
            echo "?";
        }
        echo $type->getName();
        echo $type->isBuiltin() ? " (builtin)" : " (class)";
    } else {
        echo ": mixed (no type hint)";
    }

    if ($param->isOptional()) {
        echo " [optional]";
    }

    echo "\n";
}

// `__toString()` renders PHP's own dump, which is the fastest way to see
// everything a reflector holds. The only line PHP prints that elephc does not
// is `@@ <file> <line> - <line>`: a compiled binary has no honest answer for
// it, since the source it was built from need not exist where it runs.
echo "\n", $fn;

class Inbox
{
    final protected static function route(string|int $id, int &$hits = 0, ...$rest): ?Mailer
    {
        return null;
    }
}

echo "\n", new ReflectionMethod('Inbox', 'route');

// One parameter renders on its own too.
echo "\n", (new ReflectionMethod('Inbox', 'route'))->getParameters()[1], "\n";
