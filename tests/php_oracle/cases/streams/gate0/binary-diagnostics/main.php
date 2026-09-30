<?php

$stdin = file_get_contents('php://stdin');
fwrite(STDOUT, "out\0\xff");
fwrite(STDERR, "err\0\xfe");
trigger_error('oracle warning', E_USER_WARNING);
trigger_error('oracle deprecation', E_USER_DEPRECATED);
file_put_contents('result.bin', $stdin . "\0done");
symlink('result.bin', 'result.link');

return [
    'return' => false,
    '__oracle' => [
        'reference_outputs' => ['input' => $stdin],
        'metadata' => ['stdin_length' => strlen($stdin)],
    ],
];
