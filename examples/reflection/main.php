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

// Every getType() result shares the abstract ReflectionType parent, so one
// helper typed against it describes named and union types alike.
function describeType(?ReflectionType $type): string
{
    if ($type === null) {
        return "untyped";
    }
    return get_class($type) . " " . (string) $type;
}

function pick(string|int $key, ?Mailer $mailer)
{
}

foreach ((new ReflectionFunction('pick'))->getParameters() as $param) {
    echo "  \$", $param->getName(), " -> ", describeType($param->getType()), "\n";
}
