<?php

class Address {
    public string $city = "Rome";
}

class Profile {
    public ?Address $address;
}

class User {
    public ?Profile $profile;
}

$withAddress = new User();
$profile = new Profile();
$profile->address = new Address();
$withAddress->profile = $profile;

$withoutProfile = new User();

echo $withAddress?->profile?->address?->city ?? "unknown";
echo "\n";
echo $withoutProfile?->profile?->address?->city ?? "unknown";
echo "\n";
