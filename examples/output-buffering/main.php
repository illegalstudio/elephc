<?php

// Output buffering: capture everything a piece of code prints, then decide
// what to do with it — post-process it, store it, or discard it.

// 1) Capture and transform: render a fragment, then uppercase it.
ob_start();
echo "hello from the buffer";
$fragment = ob_get_clean();
echo strtoupper($fragment), "\n";

// 2) Build a small "template" by capturing printed output into a string.
function render_row(string $name, int $qty): void
{
    printf("| %-10s | %3d |\n", $name, $qty);
}

ob_start();
echo "+------------+-----+\n";
render_row("apples", 12);
render_row("pears", 7);
echo "+------------+-----+\n";
$table = ob_get_clean();

echo "rendered ", strlen($table), " bytes:\n", $table;

// 3) Nested buffers: the inner buffer flushes into the outer one.
ob_start();
echo "outer:";
ob_start();
echo "inner";
ob_end_flush();          // "inner" flows into the outer buffer
$combined = ob_get_clean();
echo $combined, "\n";

// 4) Inspect the buffer stack while it is active.
ob_start();
echo "1234567";
$status = ob_get_status();
echo ob_get_clean(), "\n";
echo "handler=", $status["name"], " used=", $status["buffer_used"], "\n";

// 5) Discard output entirely: ob_end_clean() throws the capture away.
ob_start();
var_dump(["noisy" => "debug output"]);
ob_end_clean();
echo "debug output was discarded\n";

// 6) Output handlers: transform everything a buffer captured on flush.
ob_start(function (string $buffer, int $phase): string {
    return strtoupper($buffer);
});
echo "handlers rewrite the flushed bytes";
ob_end_flush();
echo "\n";

// 7) Chunked buffers auto-flush once they reach the chunk size.
ob_start(function (string $b, int $p): string { return "{" . $b . "}"; }, 8);
echo "12345678";
echo "tail";
ob_end_flush();
echo "\n";
