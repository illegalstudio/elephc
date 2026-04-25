<?php

class Counter {
    public static int $count = 1;
    public static string $label = "visits";

    public static function bump() {
        self::$count = self::$count + 1;
        return self::$count;
    }
}

echo Counter::$label . ":" . Counter::$count . "\n";
Counter::bump();
Counter::bump();
echo Counter::$label . ":" . Counter::$count;
