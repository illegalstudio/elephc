<?php

class Box implements ArrayAccess {
    public function offsetExists(mixed $offset): bool {
        echo "E";
        return (string)$offset === "p";
    }

    public function offsetGet(mixed $offset): mixed {
        if ((string)$offset !== "p") {
            echo "!";
        }
        echo "G";
        throw new Exception("x");
    }

    public function offsetSet(mixed $offset, mixed $value): void {
        if ((string)$offset === "" && $value === null) {
            return;
        }
    }

    public function offsetUnset(mixed $offset): void {
        if ((string)$offset === "") {
            return;
        }
    }
}

function make_key(): string {
    echo "K";
    return "p";
}

try {
    $box = new Box();
    $value = $box[make_key()];
} catch (Exception $e) {
    echo "|caught\n";
}

// empty() asks offsetExists() first; offsetGet(), which would throw here, only runs when the
// offset exists.
echo empty($box["q"]) ? "|empty\n" : "|set\n";
