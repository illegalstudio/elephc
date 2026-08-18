<?php

$pid = pcntl_fork();

if ($pid === -1) {
    echo 'Unable to fork: ' . pcntl_strerror(pcntl_get_last_error()) . "\n";
    exit(1);
}

if ($pid === 0) {
    echo "Child completed its work\n";
    exit(7);
}

$status = 0;
$usage = [];
$waited = pcntl_waitpid($pid, $status, 0, $usage);

if ($waited === $pid && pcntl_wifexited($status)) {
    echo 'Parent reaped child exit code ' . pcntl_wexitstatus($status) . "\n";
    echo 'Child user CPU seconds: ' . $usage['ru_utime.tv_sec'] . "\n";
}
