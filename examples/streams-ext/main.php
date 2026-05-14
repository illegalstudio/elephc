<?php
// Stream extensions — fgetc reads one byte at a time, readfile dumps a
// whole file to stdout, flock synchronises concurrent writers, and
// tmpfile() opens a self-deleting temporary file.

// --- fgetc: read a file character by character.
file_put_contents("greet.txt", "hi!");
$h = fopen("greet.txt", "r");
echo "fgetc: ";
while (!feof($h)) {
    $c = fgetc($h);
    if ($c !== false) {
        echo $c;
    }
}
echo "\n";
fclose($h);

// --- readfile: stream the file straight to stdout.
echo "readfile: ";
$bytes = readfile("greet.txt");
echo " (" . $bytes . " bytes)\n";

// --- flock: take an exclusive lock, write, then release.
$h = fopen("locked.txt", "w");
if (flock($h, LOCK_EX)) {
    fwrite($h, "exclusive write\n");
    flock($h, LOCK_UN);
}
fclose($h);
echo "locked file: " . file_get_contents("locked.txt");

// --- tmpfile: a unique handle that is unlinked the moment it is closed.
$tmp = tmpfile();
fwrite($tmp, "ephemeral\n");
rewind($tmp);
echo "tmpfile contents: " . fread($tmp, 64);
fclose($tmp);

unlink("greet.txt");
unlink("locked.txt");
echo "done\n";
