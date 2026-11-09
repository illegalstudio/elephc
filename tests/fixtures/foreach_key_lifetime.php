<?php
final class Entry
{
    public function __construct(public readonly int $id) {}
}

final class History
{
    private array<string, Entry> $items = [];

    public function step(): void
    {
        array<string, Entry> $next = [];
        for ($i = 0; $i < 28; $i++) {
            $key = $i . ':' . $i;
            if (isset($this->items[$key])) {
                $entry = $this->items[$key];
            } else {
                $entry = new Entry($i);
            }
            $next[$key] = $entry;
        }
        foreach ($this->items as $key => $entry) {
            if ($entry->id === -1) {
                throw new LogicException('Invalid item.');
            }
        }
        $this->items = $next;
    }

    public function close(): void
    {
        $this->items = [];
    }
}

function cycle(): void
{
    $history = new History();
    $history->step();
    $history->close();
}

function measure(): void
{
    cycle();
    for ($trial = 0; $trial < 3; $trial++) {
        $before = memory_get_usage();
        cycle();
        $after = memory_get_usage();
        echo 'net=', $after - $before, "\n";
    }
}

measure();
