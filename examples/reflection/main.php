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

// A custom reflector keeps its defaults and runs its constructor after forwarding
// the leading metadata argument unchanged to the builtin constructor.
class LabeledReflection extends ReflectionClass
{
    private string $prefix = 'class';
    public string $label = 'unlabeled';

    public function __construct(string $name, string $label)
    {
        parent::__construct($name);
        $this->label = $label;
    }

    public function describe(): string
    {
        return $this->prefix . ' ' . $this->getName() . ': ' . $this->label;
    }
}

$labeled = new LabeledReflection(Defaults::class, 'configured');
echo $labeled->describe(), "\n";
echo "Public reflector properties: ", json_encode(get_object_vars($labeled)), "\n";

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

// A formatter can inspect collection, floating-point and escaped string defaults together.
function configureReport(iterable $columns = [], float $ratio = 0.3, string $separator = "\n", int $flags = ENT_QUOTES): void
{
}

echo "\n", new ReflectionFunction('configureReport');
