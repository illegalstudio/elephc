<?php

namespace App;

// A polyfill-style class: most calls take a fast path, but a rarely-used branch lazily loads a
// data table by a *computed* path. elephc is a closed-world AOT compiler, so it cannot resolve a
// runtime-dynamic `require` at compile time.
//
// Because this class is pulled in transitively by the autoloader (it is library code, not the
// program's own entry file), elephc does NOT fail the build. Instead it degrades the unresolvable
// `require $file` into a runtime-fatal stub: the rest of the class compiles normally, and the
// stub only fires -- printing a clear message to stderr and exiting -- if that exact branch is
// ever reached at run time. Programs that never take the lazy branch compile and run unaffected.
class Normalizer
{
    // Fast path used by a typical request: no dynamic include, so nothing is degraded here.
    public static function label(string $s): string
    {
        return strtoupper($s);
    }

    // Lazy branch: loads a Unicode data table from a file chosen at run time. The `require $file`
    // cannot be resolved at compile time, so elephc compiles it as a runtime-fatal stub. It only
    // executes if loadTable() is actually called.
    public static function loadTable(string $name)
    {
        $file = __DIR__ . '/unidata/' . $name . '.php';
        return require $file;
    }
}
