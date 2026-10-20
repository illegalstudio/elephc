<?php
// A value argument can mutate the same queue used as the by-reference receiver.
class Queue
{
    public $ids = [10, 20];
}

$queue = new Queue;
$original = $queue->ids;
array_unshift($queue->ids, (int) array_shift($queue->ids));
echo "Current queue: ", implode(", ", $queue->ids), "\n";
echo "Original copy: ", implode(", ", $original), "\n";

$prepend = array_unshift(...);
$prepend($queue->ids, (int)array_shift($queue->ids));
echo "Callable queue: ", implode(", ", $queue->ids), "\n";
