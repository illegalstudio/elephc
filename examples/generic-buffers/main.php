<?php

// Storage keeps T's native width. Values passed to methods use PHP int or float.
final class NativeBuffer<T>
{
    private buffer<T> $storage;

    public function __construct(private int $length)
    {
        $this->storage = buffer_new<T>($length);
    }

    public function get(int $index): T
    {
        return $this->storage[$index];
    }

    public function set(int $index, T $value): void
    {
        $this->storage[$index] = $value;
    }

    public function fill(T $value): void
    {
        for ($i = 0; $i < $this->length; $i++) {
            $this->storage[$i] = $value;
        }
    }

    public function __destruct()
    {
        $storage = $this->storage;
        buffer_free($storage);
    }
}

$pixels = new NativeBuffer<u32>(3);
$pixels->fill(0xFF000000);
$pixels->set(1, 0xFFFFFFFF);
echo $pixels->get(0), ' ', $pixels->get(1), "\n";

$bytes = new NativeBuffer<u8>(2);
$bytes->fill(257);
echo $bytes->get(0), ' ', $bytes->get(1), "\n";

$samples = new NativeBuffer<f32>(2);
$samples->fill(0.1);
echo $samples->get(0), "\n";
