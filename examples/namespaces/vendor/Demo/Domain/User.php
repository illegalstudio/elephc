<?php

namespace Demo\Domain;

class User {
    public $name;
    public $role;

    public function __construct($name, $role) {
        $this->name = $name;
        $this->role = $role;
    }

    public function badge() {
        return "@" . $this->name . " (" . $this->role . ")";
    }
}
