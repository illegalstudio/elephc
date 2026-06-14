<?php

# PHP 8 attributes (`#[Foo]`) decorate any declaration: classes, functions,
# methods, properties, parameters, closures, and enum cases. elephc captures
# them in the AST. Most attributes are user-defined and have no compile-time
# effect, but a few are enforced/diagnosed by the type checker:
#
#   - `#[\Override]` (PHP 8.3) — the marked method must override a parent or
#     interface method, otherwise it's a compile error.
#   - `#[\Deprecated("reason")]` (PHP 8.4) — every call site emits a warning.

class Author {
    public function __construct(string $name) {
        echo "Author instance: ";
        echo $name;
        echo "\n";
    }
}

#[Author("Ada")]
#[Version(1)]
class Greeter {
    const VERSION = 1;

    #[Slot]
    public string $who;

    public function __construct(#[Required] string $who) {
        $this->who = $who;
    }

    #[Pure]
    public function greet(): void {
        echo "Hello, ";
        echo $this->who;
        echo "!\n";
    }
}

class LoudGreeter extends Greeter {
    #[\Override]
    public function greet(): void {
        echo "HELLO, ";
        echo $this->who;
        echo "!!\n";
    }
}

#[Memoized]
function double(int $x): int {
    return $x * 2;
}

$loud = new LoudGreeter("world");
$loud->greet();
echo double(7);
echo "\n";
echo "Greeter v";
echo Greeter::VERSION;
echo "\n";

// PHP 8.2 #[\AllowDynamicProperties]: undeclared property writes are routed
// to a per-object hashtable side-table at runtime.
#[\AllowDynamicProperties]
class Bag {}

$bag = new Bag();
$bag->host = "localhost";
$bag->port = 8080;
echo $bag->host;
echo ":";
echo $bag->port;
echo "\n";

// Reflection-style introspection: read class/member attribute names and
// supported literal arguments. Class and attribute names must be string
// literals (no dynamic lookup yet).
echo "Greeter attrs:";
foreach (class_attribute_names('Greeter') as $name) {
    echo " ";
    echo $name;
}
echo "\n";

echo "Author args:";
foreach (class_attribute_args('Greeter', 'Author') as $arg) {
    echo " ";
    echo $arg;
}
echo "\n";

echo "Reflection attrs:";
foreach (class_get_attributes('Greeter') as $attr) {
    echo " ";
    echo $attr->getName();
}
echo "\n";

$ref = new ReflectionClass('Greeter');
echo "ReflectionClass name: ";
echo $ref->getName();
echo "\n";
$attrs = $ref->getAttributes();
echo "ReflectionClass first attr: ";
echo $attrs[0]->getName();
echo "\n";
$attrs[0]->newInstance();

// Attribute arguments can be floats (and negated numeric literals), not just
// strings and ints. They round-trip through getArguments() unchanged.
#[Threshold(0.9, -1.5)]
class Sensor {}

$thresholds = (new ReflectionClass('Sensor'))->getAttributes()[0]->getArguments();
echo "Sensor thresholds:";
foreach ($thresholds as $value) {
    echo " ";
    echo $value;
}
echo "\n";

// Array arguments (including nested arrays) are materialized too.
#[Roles(['admin', 'editor', 'viewer'])]
class Account {}

$roles = (new ReflectionClass('Account'))->getAttributes()[0]->getArguments()[0];
echo "Account roles:";
foreach ($roles as $role) {
    echo " ";
    echo $role;
}
echo "\n";

// Named arguments are returned under their string keys, like in PHP.
#[\Attribute]
class Endpoint
{
    public function __construct(public string $path, public array $methods = []) {}
}

#[Endpoint('/users', methods: ['GET', 'POST'])]
class UserApi {}

$endpoint = (new ReflectionClass('UserApi'))->getAttributes()[0];
$endpointArgs = $endpoint->getArguments();
echo "Endpoint path: ", $endpointArgs[0], "\n";
echo "Endpoint methods: ", $endpointArgs['methods'][0], " ", $endpointArgs['methods'][1], "\n";

$endpointInstance = $endpoint->newInstance();
echo "Endpoint instance path: ", $endpointInstance->path, "\n";

// Arguments can also be symbolic references — a global constant, a class /
// interface constant, or an enum case — resolved when reflection reads them back.
// A global/class constant resolves to its value; an enum case resolves to the
// case object (just like PHP's ReflectionAttribute::getArguments()).
const DEFAULT_LIMIT = 100;

class Bounds {
    const CEILING = 999;
}

enum Tier: string {
    case Free = 'free';
    case Pro = 'pro';
}

#[Policy(DEFAULT_LIMIT, Bounds::CEILING, Tier::Pro)]
class Quota {}

$policyArgs = (new ReflectionClass('Quota'))->getAttributes()[0]->getArguments();
echo "Policy limit: ", $policyArgs[0], "\n";
echo "Policy ceiling: ", $policyArgs[1], "\n";
echo "Policy tier: ", $policyArgs[2]->value, "\n";

$method = new ReflectionMethod('Greeter', 'greet');
echo "ReflectionMethod attrs:";
foreach ($method->getAttributes() as $attr) {
    echo " ";
    echo $attr->getName();
}
echo "\n";

$property = new ReflectionProperty(Greeter::class, 'who');
echo "ReflectionProperty attrs:";
foreach ($property->getAttributes() as $attr) {
    echo " ";
    echo $attr->getName();
}
echo "\n";
