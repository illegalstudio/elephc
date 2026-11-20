<?php

// Managed timers: every callback is normalized with Closure::fromCallable() and owned by a
// typed ?Closure property. Cancelling a timer drops the closure, which releases whatever the
// callback captured (a bound receiver, a `use` variable) without any manual bookkeeping.

final class Timer {
    private ?Closure $callback;
    private int $remaining;

    public function __construct(callable $callback, private int $interval, private bool $repeat) {
        $this->callback = Closure::fromCallable($callback);
        $this->remaining = $interval;
    }

    public function active(): bool {
        return $this->callback !== null;
    }

    public function cancel(): void {
        $this->callback = null;
    }

    public function advance(int $elapsed): void {
        $callback = $this->callback;
        if ($callback === null) {
            return;
        }
        $this->remaining -= $elapsed;
        if ($this->remaining > 0) {
            return;
        }
        $callback($this);
        if ($this->repeat) {
            $this->remaining += $this->interval;
        } else {
            $this->cancel();
        }
    }
}

final class TimerQueue {
    /** @var Timer[] */
    private array $timers = [];

    public function after(int $milliseconds, callable $callback): Timer {
        return $this->add(new Timer($callback, $milliseconds, false));
    }

    public function every(int $milliseconds, callable $callback): Timer {
        return $this->add(new Timer($callback, $milliseconds, true));
    }

    public function advance(int $elapsed): void {
        foreach ($this->timers as $timer) {
            $timer->advance($elapsed);
        }
        $alive = [];
        foreach ($this->timers as $timer) {
            if ($timer->active()) {
                $alive[] = $timer;
            }
        }
        $this->timers = $alive;
    }

    public function pending(): int {
        return count($this->timers);
    }

    private function add(Timer $timer): Timer {
        $this->timers[] = $timer;
        return $timer;
    }
}

final class Door {
    public function __construct(private string $name) {}

    public function __destruct() {
        echo "  ({$this->name} released)\n";
    }

    public function close(Timer $timer): void {
        echo "  {$this->name} closes\n";
    }

    public function name(): string {
        return $this->name;
    }
}

function autosave(Timer $timer): void {
    echo "  autosave\n";
}

function play(): void {
    $queue = new TimerQueue();

    // A named function.
    $queue->every(200, "autosave");

    // A callable array: the timer keeps the receiver alive after the local is gone.
    $vault = new Door("vault door");
    $queue->after(300, [$vault, "close"]);
    unset($vault);

    // An existing closure that cancels its own timer while it is running.
    $blinks = 0;
    $queue->every(100, function (Timer $timer) use (&$blinks): void {
        $blinks++;
        echo "  blink {$blinks}\n";
        if ($blinks === 3) {
            $timer->cancel();
        }
    });

    // A timer cancelled from outside before it ever fires.
    $gate = new Door("gate");
    $alarm = $queue->after(1000, function (Timer $timer) use ($gate): void {
        echo "  {$gate->name()} alarm\n";
    });
    unset($gate);

    for ($clock = 100; $clock <= 500; $clock += 100) {
        echo "t={$clock}ms\n";
        $queue->advance(100);
        if ($clock === 400) {
            echo "  cancelling the alarm\n";
            $alarm->cancel();
        }
    }
    echo "pending timers: " . $queue->pending() . "\n";
}

play();
echo "done\n";
