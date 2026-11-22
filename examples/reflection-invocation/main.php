<?php
// Compile with: elephc build --php-only --strict-coercions examples/reflection-invocation/main.php

// Invocation consumes the current array, including entries added after construction.
function renderDelivery(string $address, string $subject = 'Update'): string
{
    return $subject . ': ' . $address;
}

$delivery = ['address' => 'team@example.com'];
$delivery['subject'] = 'Build status';
$renderer = new ReflectionFunction('renderDelivery');
echo "\nInvoked delivery: ", $renderer->invokeArgs($delivery), "\n";

class Delivery
{
    public string $address;
    public function __construct(string $address) { $this->address = $address; }
}

$type = new ReflectionClass('Delivery');
$instance = $type->newInstanceArgs(['address' => $delivery['address']]);
echo 'Constructed delivery: ', $instance->address, "\n";
