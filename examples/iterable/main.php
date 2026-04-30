<?php

function print_iterable(iterable $items): void {
    foreach ($items as $key => $value) {
        echo $key;
        echo '=';
        echo $value;
        echo "\n";
    }
}

print_iterable([10, 20, 30]);
print_iterable(['first' => 1, 'second' => 2]);
echo is_iterable([1]) ? "iterable\n" : "not iterable\n";
