<?php

// Keep the example's working directory and output descriptors so its result is visible.
if (!pcntl_daemon(no_chdir: true, no_close: true)) {
    echo 'Unable to daemonize: ' . pcntl_strerror(pcntl_get_last_error()) . "\n";
    exit(1);
}

echo "Daemon process is running\n";
