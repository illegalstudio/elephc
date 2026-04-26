<?php

class Counter {
    public static int $count = 1;
    public static string $label = "visits";
    public static array $history = [];

    public static function bump() {
        self::$count = self::$count + 1;
        self::$history[] = self::$count;
        return self::$count;
    }
}

class TenantCounter extends Counter {
    public static int $count = 10;
    public static array $history = [];

    public static function localBump() {
        static::$count = static::$count + 1;
        static::$history[] = static::$count;
        return static::$count;
    }
}

echo Counter::$label . ":" . Counter::$count . "\n";
Counter::bump();
Counter::bump();
Counter::$history[1] = 7;
TenantCounter::localBump();
echo Counter::$label . ":" . Counter::$count . ":" . Counter::$history[1] . "\n";
echo "tenant:" . TenantCounter::$count . ":" . TenantCounter::$history[0];
