<?php

interface Identifiable
{
    public function id(): int;
}

interface Timestamped
{
    public function createdAt(): int;
}

class Record implements Identifiable, Timestamped
{
    public function __construct(private int $id, private int $time) {}

    public function id(): int
    {
        return $this->id;
    }

    public function createdAt(): int
    {
        return $this->time;
    }
}

// An intersection parameter type `A&B`: the argument must implement both interfaces.
function summarize(Identifiable&Timestamped $record): int
{
    return $record->id();
}

echo summarize(new Record(42, 1000)), "\n";
