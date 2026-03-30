<?php

interface Named {
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
    public function name() {
        return "widget";
    }
}

$printer = new ProductPrinter();
$printer->printLine();
