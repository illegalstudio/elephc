<?php

// A terminal app decides how to render from whether its output is a terminal,
// then signals a worker process and reports failures through ext/posix's errno.

$interactive = posix_isatty(STDOUT);
echo $interactive
    ? "stdout is a terminal: using colors\n"
    : "stdout is not a terminal: plain output\n";

echo 'Effective user id: ' . posix_geteuid() . "\n";
echo 'Process group id: ' . posix_getpgrp() . "\n";

$worker = pcntl_fork();
if ($worker === -1) {
    // posix_kill(-1, ...) would signal every process this user may signal.
    echo "Could not fork a worker\n";
    exit(1);
}
if ($worker === 0) {
    sleep(30);
    exit(0);
}

if (posix_kill($worker, SIGTERM)) {
    pcntl_waitpid($worker, $status);
    if (pcntl_wifsignaled($status)) {
        echo 'Worker stopped by signal ' . pcntl_wtermsig($status) . "\n";
    }
}

if (!posix_kill(2147483647, 0)) {
    $error = posix_get_last_error();
    echo "Probing a missing process failed: errno {$error} (" . posix_strerror($error) . ")\n";
}
