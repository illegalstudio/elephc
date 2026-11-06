<?php
namespace Elephc\Web {

/** An owned snapshot of the current HTTP request's basic metadata and body. */
final class Request
{
    public function __construct(
        public string $method,
        public string $uri,
        public string $path,
        public string $body,
    ) {}
}

/** Implemented by a framework adapter or a standalone persistent application. */
interface WorkerApplication
{
    public function handle(Request $request): void;
    public function shutdown(): void;
}
}
