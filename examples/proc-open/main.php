<?php
// Run child processes with proc_open(): talk to them over pipes, watch their
// status, and collect their exit codes.

// 1. Feed `sort` through its stdin pipe and read the sorted lines back.
$sort = proc_open(['sort'], [0 => ['pipe', 'r'], 1 => ['pipe', 'w']], $pipes);
fwrite($pipes[0], "pear\napple\nfig\n");
fclose($pipes[0]);
echo "sorted:\n", stream_get_contents($pipes[1]);
fclose($pipes[1]);
echo "sort exited with ", proc_close($sort), "\n";

// 2. A shell command with its own working directory and environment; stderr is
//    merged into the stdout pipe.
$shell = proc_open(
    'echo "hello from $NAME in $(pwd)"; echo "warning" >&2; exit 4',
    [1 => ['pipe', 'w'], 2 => ['redirect', 1]],
    $pipes,
    '/',
    ['NAME' => 'elephc'],
);
echo stream_get_contents($pipes[1]);
fclose($pipes[1]);

// The exit code is reported by proc_get_status() once the child is reaped, and
// cached, so proc_close() still returns it afterwards.
do {
    usleep(10000);
    $status = proc_get_status($shell);
} while ($status['running']);
echo "pid ", $status['pid'] > 0 ? "known" : "unknown", ", exit code ", $status['exitcode'], "\n";
echo "proc_close() says ", proc_close($shell), "\n";

// 3. Stop a long-running child with proc_terminate().
$sleeper = proc_open(['sleep', '30'], [], $pipes);
proc_terminate($sleeper);
do {
    usleep(10000);
    $status = proc_get_status($sleeper);
} while ($status['running']);
echo "sleep was stopped by signal ", $status['termsig'], "\n";
proc_close($sleeper);
