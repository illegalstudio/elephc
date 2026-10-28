<?php
// A managed owner keeps checked access and closes all aliases together.
final class CheckedBuffer<T>
{
    private buffer<T> $storage;
    private bool $open = false;

    public function __construct(private readonly int $size)
    {
        if ($size < 0) { throw new InvalidArgumentException('negative length'); }
        $this->storage = buffer_new<T>($size);
        $this->open = true;
    }

    public function __destruct() { $this->close(); }
    private function __clone() {}

    public function get(int $index): T
    {
        $this->assertIndex($index);
        return $this->storage[$index];
    }

    public function set(int $index, T $value): void
    {
        $this->assertIndex($index);
        $this->storage[$index] = $value;
    }

    public function close(): void
    {
        if (!$this->open) { return; }
        $this->open = false;
        $storage = $this->storage;
        buffer_free($storage);
    }

    private function assertOpen(): void
    {
        if (!$this->open) { throw new LogicException('closed'); }
    }

    private function assertIndex(int $index): void
    {
        $this->assertOpen();
        if ($index < 0 || $index >= $this->size) {
            throw new OutOfRangeException('index:' . $index . '/' . $this->size);
        }
    }
}

function updateChecked(CheckedBuffer<int> $values, int $count, int $frames): void
{
    for ($frame = 0; $frame < $frames; $frame++) {
        for ($i = 0; $i < $count; $i++) {
            $values->set($i, ($values->get($i) + $frame + $i) & 65535);
        }
    }
}

function demonstrateChecked(int $count): int
{
    $values = new CheckedBuffer<int>($count);
    for ($i = 0; $i < $count; $i++) { $values->set($i, $i & 255); }
    updateChecked($values, $count, 3);
    $sum = 0;
    for ($i = 0; $i < $count; $i++) { $sum += $values->get($i); }
    return $sum;
}

echo demonstrateChecked($argc + 3), "\n";
