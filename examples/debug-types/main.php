<?php

// get_debug_type() reports the resolved runtime type: scalars, containers,
// class names, and the concrete resource kind. It is the diagnostic helper
// used by PHP itself when building exception messages.

class Article
{
    public function __construct(public string $title)
    {
    }
}

enum Status
{
    case Draft;
}

$values = [
    42,
    1.5,
    "hello",
    true,
    null,
    [1, 2, 3],
    new Article("Release notes"),
    Status::Draft,
    function (): void {
    },
];

foreach ($values as $value) {
    echo get_debug_type($value), "\n";
}

$stream = fopen("php://temp", "r+");
$context = stream_context_create();
$filter = stream_filter_append($stream, "string.toupper", STREAM_FILTER_WRITE);

echo get_debug_type($stream), "\n";
echo get_debug_type($context), "\n";
echo get_debug_type($filter), "\n";

fclose($stream);
echo get_debug_type($stream), "\n";
