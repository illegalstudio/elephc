<?php

// An optional global debug helper, defined eagerly (composer "files" autoload) and guarded the
// way polyfills are. Its body references the heavy App\Dumper. elephc treats `dump` as an
// optional helper: because main.php never calls dump(), this definition is pruned before
// class-reference collection, so App\Dumper is never pulled into the closed-world binary.
// (This is exactly how Symfony's u()/b()/dump() drag in UnicodeString/ByteString/VarDumper on a
// render path that never calls them.)
if (!function_exists('dump')) {
    function dump($value): void
    {
        \App\Dumper::render($value);
    }
}
