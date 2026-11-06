<?php
// This bootstrap owns terminal I/O; every submitted PHP fragment runs in eval.
extern "elephc_magician" {
    function __elephc_repl_next(): int;
    function __elephc_repl_source(): string;
    function __elephc_repl_failed(): void;
    function __elephc_repl_status(): int;
}

while (__elephc_repl_next()) {
    try {
        eval(__elephc_repl_source());
    } catch (Throwable $__elephc_repl_error) {
        fwrite(STDERR, get_class($__elephc_repl_error) . ": " . $__elephc_repl_error->getMessage() . "\n");
        unset($__elephc_repl_error);
        __elephc_repl_failed();
    }
}
exit(__elephc_repl_status());
