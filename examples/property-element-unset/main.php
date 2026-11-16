<?php
// Removing elements from arrays held in properties: an event registry that
// forgets listeners, a shared static cache, and a small ArrayAccess settings
// bag whose offsetUnset() is reached through a property.

class Settings implements ArrayAccess
{
    private array $values = [];

    public function offsetExists(mixed $key): bool { return isset($this->values[$key]); }
    public function offsetGet(mixed $key): mixed { return $this->values[$key] ?? null; }
    public function offsetSet(mixed $key, mixed $value): void { $this->values[$key] = $value; }
    public function offsetUnset(mixed $key): void { unset($this->values[$key]); }
    public function keys(): array { return array_keys($this->values); }
}

class EventRegistry
{
    public static array $lookups = [];
    public Settings $settings;
    private array $listeners = [];

    public function __construct()
    {
        $this->settings = new Settings();
    }

    public function on(string $event, string $listener): void
    {
        $this->listeners[$event] = $listener;
        self::$lookups[$event] = strlen($listener);
    }

    public function off(string $event): void
    {
        // Each of these removes one key and keeps the others' keys as they are.
        unset($this->listeners[$event], self::$lookups[$event]);
    }

    public function events(): array
    {
        return array_keys($this->listeners);
    }
}

$registry = new EventRegistry();
$registry->on("boot", "warmCache");
$registry->on("request", "route");
$registry->on("shutdown", "flushLogs");
$registry->off("request");
$registry->off("never-registered");

echo "events: ", implode(", ", $registry->events()), "\n";
echo "cached lookups: ", implode(", ", array_keys(EventRegistry::$lookups)), "\n";

$registry->settings["debug"] = true;
$registry->settings["locale"] = "en";
unset($registry->settings["debug"]);
echo "settings: ", implode(", ", $registry->settings->keys()), "\n";

// A list keeps its surviving indexes: removing one leaves a hole, no renumbering.
$scores = new class {
    public array $values = [90, 75, 60];
};
unset($scores->values[1]);
foreach ($scores->values as $index => $score) {
    echo "score #$index = $score\n";
}
