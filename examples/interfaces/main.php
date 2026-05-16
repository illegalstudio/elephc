<?php

interface Named {
    public string $name { get; }

    public function name();
}

interface Labeled extends Named {
    public function label();
}

abstract class BasePrinter implements Labeled {
    public function label() {
        return strtoupper($this->name());
    }

    public function printLine() {
        echo $this->label() . "\n";
    }
}

class ProductPrinter extends BasePrinter {
    public string $name = "widget";

    public function name() {
        return $this->name;
    }
}

$printer = new ProductPrinter();
$printer->printLine();
