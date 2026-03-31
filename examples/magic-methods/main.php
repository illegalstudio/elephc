<?php

class User {
    public $name;
    public $log = "";

    public function __construct($name) {
        $this->name = $name;
    }

    public function __toString() {
        return "@" . $this->name;
    }

    public function __get($name) {
        return "[" . $name . "]";
    }

    public function __set($name, $value) {
        $this->log = $this->log . $name . "=" . $value . ";";
    }
}

$user = new User("nahime");
$user->role = "admin";
$user->visits = 3;

echo $user . "\n";
echo $user->missing . "\n";
echo $user->log . "\n";
