<?php

$list = new SplDoublyLinkedList();
$list->push("a");
$list->push("b");
$list->push("c");
$list->setIteratorMode(SplDoublyLinkedList::IT_MODE_FIFO | SplDoublyLinkedList::IT_MODE_DELETE);

foreach ($list as $key => $value) {
    echo $key . ":" . $value . "|";
    if ($value === "a") {
        $list->push("x");
    }
}

echo "\ncount=" . count($list) . "\n";
