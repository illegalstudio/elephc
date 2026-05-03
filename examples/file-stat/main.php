<?php
// Extended file metadata — beyond the basic file_exists/filesize/filemtime
// triplet, elephc exposes the rest of the stat() field set as individual
// helpers, plus stat()/lstat()/fstat() that return the full record as an
// array on success.

file_put_contents("data.txt", "hello\n");

// One-shot helpers, each backed by the same stat buffer.
echo "perms (octal): " . sprintf("%04o", fileperms("data.txt") & 0o777) . "\n";
echo "type:          " . filetype("data.txt") . "\n";
echo "owner uid:     " . fileowner("data.txt") . "\n";
echo "group gid:     " . filegroup("data.txt") . "\n";
echo "inode:         " . fileinode("data.txt") . "\n";

$atime = fileatime("data.txt");
$ctime = filectime("data.txt");
$mtime = filemtime("data.txt");
echo "atime <= mtime? " . ($atime <= $mtime ? "y" : "n") . "\n";
echo "ctime <= mtime? " . ($ctime <= $mtime ? "y" : "n") . "\n";

// is_executable / is_link probe specific bits.
echo "is_executable: " . (is_executable("data.txt") ? "y" : "n") . "\n";
echo "is_link:       " . (is_link("data.txt") ? "y" : "n") . "\n";

// Full stat() array — both the integer and the named keys are populated,
// matching PHP's classic dual indexing.
$info = stat("data.txt");
if ($info !== false) {
    echo "stat[size]:  " . $info["size"] . "\n";
    echo "stat[mode] high nibble (file type): " . sprintf("%X", ($info["mode"] >> 12) & 0xF) . "\n";
}

// clearstatcache() is a no-op in elephc (no cache to clear) but is kept
// for source-level compatibility.
clearstatcache();

unlink("data.txt");
echo "done\n";
