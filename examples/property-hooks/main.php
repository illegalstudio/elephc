<?php

// Property hooks let a property compute its value on read, or validate/normalize a value on write,
// without writing separate getter and setter methods. The hook body runs on each access.

class Person
{
    public function __construct(
        private string $firstName,
        private string $lastName,
    ) {}

    // A read-only virtual property: computed on every read, with no stored value of its own.
    public string $fullName {
        get => $this->firstName . " " . $this->lastName;
    }

    // Another read-only hook, layered on top of the first.
    public string $shoutName {
        get => strtoupper($this->fullName);
    }
}

class Thermostat
{
    private float $c = 0.0;

    // Read and write hooks over a private backing field.
    public float $celsius {
        get => $this->c;
        set { $this->c = $value; }
    }

    // A derived property: writing a Fahrenheit value stores the equivalent Celsius.
    public float $fahrenheit {
        get => $this->c * 9.0 / 5.0 + 32.0;
        set { $this->c = ($value - 32.0) * 5.0 / 9.0; }
    }
}

$jane = new Person("Jane", "Doe");
echo $jane->fullName, "\n";
echo $jane->shoutName, "\n";

$t = new Thermostat();
$t->fahrenheit = 212.0;
echo "celsius=", $t->celsius, ", fahrenheit=", $t->fahrenheit, "\n";
