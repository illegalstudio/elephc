<?php

$message = "hello preg";
if (preg_match("/^hello preg$/", $message)) {
    echo "Hello, preg!\n";
}

preg_match_all('/\b/u', $message, $boundaries, PREG_OFFSET_CAPTURE);
echo 'Word boundaries:';
foreach ($boundaries[0] as $boundary) {
    echo ' ', $boundary[1];
}
echo "\n";
