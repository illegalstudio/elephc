<?php
// PHP 8.6 stream error handling. Compile with --php-version=8.6.

$silent = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Silent,
        'error_store' => StreamErrorStore::All,
    ],
]);

$stream = fopen('php://missing-stream', 'r', false, $silent);
var_dump($stream);

foreach (stream_last_errors() as $error) {
    echo $error->code->name, ': ', $error->wrapperName, ' - ', $error->message, "\n";
}
stream_clear_errors();

$terminal = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Exception,
    ],
]);

try {
    fopen('php://missing-stream', 'r', false, $terminal);
} catch (StreamException $exception) {
    echo $exception::class, ': ', $exception->getMessage(), "\n";
    echo 'stored: ', count($exception->getErrors()), " error(s)\n";
}
