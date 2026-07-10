<?php

interface Named {
    public string $name { get; }

    public static function kind();

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

    #[\Override]
    public static function kind() {
        return "product";
    }

    public function name() {
        return $this->name;
    }
}

$printer = new ProductPrinter();
$printer->printLine();
echo ProductPrinter::kind() . "\n";

class OneSlotMap implements ArrayAccess {
    private string $value = "";

    public function offsetExists(mixed $offset): bool {
        if ((string)$offset === "") {
            return false;
        }
        return $this->value !== "";
    }

    public function offsetGet(mixed $offset): mixed {
        if ((string)$offset === "") {
            return "";
        }
        return $this->value;
    }

    public function offsetSet(mixed $offset, mixed $value): void {
        if ((string)$offset === "") {
            return;
        }
        $this->value = (string)$value;
    }

    public function offsetUnset(mixed $offset): void {
        if ((string)$offset === "") {
            return;
        }
        $this->value = "";
    }
}

$map = new OneSlotMap();
$map["sku"] = "A-42";
echo $map["sku"] . "\n";
echo isset($map["sku"]) . "\n";
unset($map["sku"]);
echo isset($map["sku"]) . "\n";
