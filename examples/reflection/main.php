<?php

// Reflecting over declarations and array-valued constants at compile time.
//
// elephc resolves reflection in a closed world: ReflectionFunction,
// ReflectionParameter, and ReflectionClass read metadata baked from
// declarations, so signatures and constant values are available without
// runtime interpretation.

class Mailer {}

class Defaults {
    public const OPTIONS = ["retry" => 3, "backoff" => [100, 250, 500]];
}

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

$class = new ReflectionClass(Defaults::class);
$constant = $class->getReflectionConstant("OPTIONS");
echo "Reflected options: ", json_encode($constant->getValue()), "\n";
echo "All constants: ", json_encode($class->getConstants()), "\n";
