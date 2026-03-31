<?php

namespace Demo\Lib;

const APP_NAME = "elephc namespaces";

function render($value) {
    return "[" . $value . "]";
}

class User {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function label() {
        return "@" . $this->name;
    }
}
