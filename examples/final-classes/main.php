<?php

final class InvoiceNumber {
    public $prefix = "invoice";
    final public $value = 42;

    final public function label() {
        return $this->prefix . ":" . $this->value;
    }
}

$number = new InvoiceNumber();
echo $number->label();
echo "\n";
