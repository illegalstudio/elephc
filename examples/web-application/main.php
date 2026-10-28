<?php
require_once __DIR__ . '/application.php';

// This entry point is the ElephC-specific bootstrap, executed once after fork.
Elephc\Web\Worker::register(new CounterApplication());
