<?php

$list = new SplDoublyLinkedList();
$list->push("alpha");
$list->push("beta");
$list->unshift("start");

foreach ($list as $index => $value) {
    echo $index;
    echo ": ";
    echo $value;
    echo "\n";
}

$queue = new SplQueue();
$queue->enqueue("first");
$queue->enqueue("second");
echo "queue: ";
echo $queue->dequeue();
echo "\n";

$stack = new SplStack();
$stack->push(10);
$stack->push(20);
echo "stack: ";
echo $stack->pop();
echo "\n";

$fixed = new SplFixedArray(2);
$fixed[0] = "left";
$fixed[1] = "right";
echo "fixed: ";
echo $fixed[0];
echo ", ";
echo $fixed[1];
echo "\n";

$max = new SplMaxHeap();
$min = new SplMinHeap();
foreach ([3, 1, 5, 2] as $value) {
    $max->insert($value);
    $min->insert($value);
}

echo "max heap: ";
while (!$max->isEmpty()) {
    echo $max->extract();
}
echo "\n";

echo "min heap: ";
foreach ($min as $value) {
    echo $value;
}
echo "\n";

$priority = new SplPriorityQueue();
$priority->insert("low", 1);
$priority->insert("high", 5);
$priority->insert("mid", 3);
$priority->setExtractFlags(SplPriorityQueue::EXTR_BOTH);
$task = $priority->extract();
echo "priority: ";
echo $task["data"];
echo " ";
echo $task["priority"];
echo "\n";

class Job {}

$left = new Job();
$right = new Job();
$storage = new SplObjectStorage();
$storage->attach($left, "left");
$storage[$right] = "right";

echo "object storage: ";
foreach ($storage as $job) {
    echo $storage[$job];
    echo " ";
}
echo count($storage);
echo "\n";
