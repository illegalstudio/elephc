<?php

// Pulled in via `require dirname(__DIR__) . '/lib/helper.php';` from public/main.php.
// `dirname(__DIR__)` folds to the project root (the parent of public/), so this
// file is resolved as <root>/lib/helper.php at compile time.
function helper(): string {
    return "from-helper";
}