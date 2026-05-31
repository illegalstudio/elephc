<?php

class Box {
    public ?Leaf $leaf = null;
}

class Leaf {
    public function run(string $s): string {
        return $s;
    }
}

function noisy(): string {
    echo "N";
    return "ok";
}

$none = null;
$box = new Box();

echo $none?->leaf?->run(noisy()) ?? "x";
echo "\n";

$box->leaf = new Leaf();
echo $box?->leaf?->run(noisy()) ?? "x";
