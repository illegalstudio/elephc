<?php

// The Symfony front-controller pattern: nearly every PHP entry point starts
// with `require dirname(__DIR__) . '/vendor/autoload.php';`. elephc folds
// `dirname(__DIR__)` at compile time when its argument is a compile-time-constant
// string (and the optional `$levels` argument is an integer literal >= 1), so
// this resolves to the project root and the include loads a sibling directory's
// file regardless of where the compiled binary is launched from.
//
// Here `public/main.php` is one level deep, so `dirname(__DIR__)` folds to the
// example root and `dirname(__DIR__) . '/lib/helper.php'` resolves to
// `<root>/lib/helper.php` — the same shape as Symfony's `public/index.php`
// reaching `<root>/vendor/autoload.php`.
require dirname(__DIR__) . '/lib/helper.php';

echo helper() . "\n";