<?php

$pid = pcntl_fork();

if ($pid === -1) {
    echo 'Unable to fork: ' . pcntl_strerror(pcntl_get_last_error()) . "\n";
    exit(1);
}

if ($pid === 0) {
    $session = posix_setsid();
    echo $session > 0
        ? "Child created session {$session}\n"
        : 'Unable to create child session: ' . pcntl_strerror(pcntl_get_last_error()) . "\n";
    exit(7);
}

$status = 0;
$usage = [];
$waited = pcntl_waitpid($pid, $status, 0, $usage);

if ($waited === $pid && pcntl_wifexited($status)) {
    echo 'Parent reaped child exit code ' . pcntl_wexitstatus($status) . "\n";
    echo 'Child user CPU seconds: ' . $usage['ru_utime.tv_sec'] . "\n";
}

$groupPid = pcntl_fork();

if ($groupPid === 0) {
    echo posix_setpgid(0, 0)
        ? "Second child created its own process group\n"
        : 'Unable to create child process group: ' . pcntl_strerror(pcntl_get_last_error()) . "\n";
    exit(0);
}

pcntl_waitpid($groupPid, $groupStatus);

$signalSet = ['user-signal' => (string) SIGUSR1];
$oldMask = [];
if (pcntl_sigprocmask(SIG_BLOCK, $signalSet, $oldMask)) {
    echo "Blocked SIGUSR1 from an associative numeric-string signal set\n";
    pcntl_sigprocmask(SIG_SETMASK, $oldMask);
}
