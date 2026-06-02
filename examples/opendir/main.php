<?php
// opendir() returns a directory handle. readdir() walks the entries one at a
// time, returning false once the directory is exhausted. rewinddir() rewinds
// the handle back to its first entry, and closedir() releases it.

$dir = opendir(".");

echo "Entries in the current directory:\n";
$count = 0;
while (($entry = readdir($dir)) !== false) {
    echo "  " . $entry . "\n";
    $count = $count + 1;
}
echo "Total: " . $count . " entries\n";

rewinddir($dir);
echo "After rewinddir(), first entry again: " . readdir($dir) . "\n";

closedir($dir);
