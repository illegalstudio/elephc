<?php

// Untyped containers hand back `mixed`: an array element, a WeakMap lookup. PHP lets such
// a value reach a class-typed return, parameter or property and checks its class at run time.

final class Timer {
    public function __construct(public readonly string $id) {}
}

final class TimerQueue {
    /** @var array<int, Timer> */
    private array $timers = [];

    /** @var array<int, mixed> */
    private array $untrusted = [];

    public function add(Timer $timer): void { $this->timers[] = $timer; }

    public function addRaw(mixed $value): void { $this->untrusted[] = $value; }

    // The element read is `mixed`; the `?Timer` return checks it.
    public function extract(): ?Timer { return $this->timers[0] ?? null; }

    public function extractRaw(): ?Timer { return $this->untrusted[0] ?? null; }
}

final class Scheduler {
    public ?Timer $next = null;

    // `object|null` is wider than `?Timer`: the property store checks it.
    public function park(object|null $timer): void { $this->next = $timer; }
}

function describe(Timer $timer): string { return "timer " . $timer->id; }

$queue = new TimerQueue();
$queue->add(new Timer("a"));
echo $queue->extract()?->id, "\n";

$bad = new TimerQueue();
$bad->addRaw("not a timer");
try {
    $bad->extractRaw();
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

$scheduler = new Scheduler();
$scheduler->park(new Timer("b"));
echo $scheduler->next?->id, "\n";
try {
    $scheduler->park(new ArrayObject([]));
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

$slots = [new Timer("c"), 42];
foreach ($slots as $slot) {
    try {
        echo describe($slot), "\n";
    } catch (TypeError $e) {
        echo explode(", called in", $e->getMessage())[0], "\n";
    }
}
