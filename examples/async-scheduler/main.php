<?php

use Elephc\Async\CancelledException;
use Elephc\Async\TaskGroup;
use function Elephc\Async\run;

$results = run(function (TaskGroup $tasks): array {
    $left = $tasks->spawn(
        fn (int $value): int => $value * 2,
        21,
    );
    $right = $tasks->spawn(
        fn (string $name): string => "hello " . $name,
        "elephc",
    );

    return [$left->await(), $right->await()];
});

echo $results[0] . "|" . $results[1] . "\n";

$events = [];
run(function (TaskGroup $tasks) use (&$events): void {
    $child = $tasks->spawn(function () use (&$events, $tasks): void {
        $events[] = "started";
        try {
            $tasks->sleep(60.0);
        } catch (CancelledException) {
            $events[] = "cleaned";
        }
    });

    $tasks->reschedule();
    $tasks->cancel();
    $child->await();
});

echo implode("|", $events) . "\n";

$ioEvents = [];
run(function (TaskGroup $tasks) use (&$ioEvents): void {
    $pair = stream_socket_pair(1, 1, 0);
    $writer = $pair[0];
    $reader = $pair[1];
    stream_set_blocking($writer, false);
    stream_set_blocking($reader, false);

    $readTask = $tasks->spawn(function () use (&$ioEvents, $tasks, $reader): void {
        $tasks->awaitReadable($reader);
        $ioEvents[] = "read:" . fread($reader, 4);
    });
    $writeTask = $tasks->spawn(function () use (&$ioEvents, $writer): void {
        $ioEvents[] = "write";
        fwrite($writer, "ping");
    });

    $readTask->await();
    $writeTask->await();
    fclose($writer);
    fclose($reader);
});

echo implode("|", $ioEvents) . "\n";
