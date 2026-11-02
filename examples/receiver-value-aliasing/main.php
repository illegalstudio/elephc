<?php
class ReceiverCounter {
    public int $value;

    public function __construct(int $value) { $this->value = $value; }

    public function readAndAdvance(int &$step): int {
        $step += 1;
        return $this->value + $step;
    }

    public function replace(ReceiverCounter &$other): void {
        $other = new ReceiverCounter(9);
    }
}

function visit(ReceiverCounter $receiver, int $count): void {
    $original = $receiver;
    $sum = 0;
    $step = 0;
    for ($index = 0; $index < $count; $index++) {
        $sum += $receiver->value;
        $sum += $receiver->readAndAdvance($step);
    }
    $receiver->replace($receiver);
    echo $sum, ':', $step, ':', $receiver->value, ':', $original->value;
}

visit(new ReceiverCounter(3), $argc + 2);
