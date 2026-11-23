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

// Walk the subject token by token: `$offset` resumes the search where the last
// token ended, and PREG_OFFSET_CAPTURE reports where each token starts.
$offset = 0;
echo 'Tokens:';
while (preg_match('/\w+/', $message, $token, PREG_OFFSET_CAPTURE, $offset) === 1) {
    echo ' ', $token[0][0], '@', $token[0][1];
    $offset = $token[0][1] + strlen($token[0][0]);
}
echo "\n";
