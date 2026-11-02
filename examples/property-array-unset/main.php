<?php

final class DeferredValues
{
    private array $values = [];

    public function add(int $key, int $value): void
    {
        $this->values[$key] = $value;
    }

    public function remove(int $key): void
    {
        unset($this->values[$key]);
    }

    public function contains(int $key): bool
    {
        return isset($this->values[$key]);
    }
}

$deferred = new DeferredValues();
$deferred->add(0, 1);
$deferred->add(1, 2);
echo ($deferred->contains(0) ? 'present-before' : 'missing-before')."\n";
echo ($deferred->contains(1) ? 'present-before' : 'missing-before')."\n";
$deferred->remove(0);

echo ($deferred->contains(0) ? 'present' : 'removed')."\n";
echo ($deferred->contains(1) ? 'present' : 'removed')."\n";
