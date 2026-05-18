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

    public function __invoke($suffix) {
        return $this->name . ":" . $suffix;
    }

    public function __call($method, $args) {
        return "missing " . $method . "(" . $args[0] . ")";
    }
}

$user = new User("nahime");
$user->role = "admin";
$user->visits = 3;

echo $user . "\n";
echo $user->missing . "\n";
echo $user->log . "\n";
echo $user("active") . "\n";
echo $user->displayName("short") . "\n";
