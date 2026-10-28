<?php
use Elephc\Web\Request;
use Elephc\Web\WorkerApplication;

/** Ordinary PHP application code, independent of the native server adapter. */
final class CounterApplication implements WorkerApplication
{
    private int $requests = 0;
    private string $previousPath = '';

    public function handle(Request $request): void
    {
        $this->requests++;
        header('Content-Type: text/plain');
        echo 'worker=', getmypid(), "\n";
        echo 'application requests=', $this->requests, "\n";
        echo 'previous path=', $this->previousPath, "\n";
        echo 'current path=', $request->path, "\n";
        echo 'query value=', $_GET['value'] ?? 'absent', "\n";
        $this->previousPath = $request->path;
    }

    public function shutdown(): void
    {
        error_log('Stopping application after ' . $this->requests . ' requests');
    }
}
