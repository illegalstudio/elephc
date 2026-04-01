<?php

readonly class Endpoint {
    public function name() {
        return "health";
    }
}

class HttpStatus {
    public static function decorate($code) {
        return "[" . status_label($code) . "]";
    }
}

function status_label($code) {
    return match($code) {
        200 => "ok",
        404 => "missing",
        500 => "error",
        default => "unknown",
    };
}

$endpoint = new Endpoint();
$labeler = status_label(...);
$decorator = HttpStatus::decorate(...);

echo $endpoint->name();
echo ":";
echo $decorator(200);
echo ":";
echo $labeler(404);
