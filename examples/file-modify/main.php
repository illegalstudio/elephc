<?php
// File modification — touch creates or stamps a file, chmod adjusts
// permissions, ftruncate resizes, fsync forces a flush to disk, umask
// controls the default permission mask of newly-created files.

// touch() creates an empty file if it does not exist, or updates the
// modification time if it does.
touch("hello.txt");
echo "exists: " . (file_exists("hello.txt") ? "y" : "n") . "\n";

// Write something and then resize via ftruncate.
file_put_contents("hello.txt", "abcdefghij");
$h = fopen("hello.txt", "r+");
ftruncate($h, 5);
fclose($h);
echo "after truncate: " . file_get_contents("hello.txt") . " (" . filesize("hello.txt") . " bytes)\n";

// fsync() forces a buffered write to disk; useful for crash safety.
$h = fopen("hello.txt", "a");
fwrite($h, "X");
fsync($h);
fclose($h);
echo "after fsync append: " . file_get_contents("hello.txt") . "\n";

// chmod() flips permissions. Use a known mask, then probe is_writable.
chmod("hello.txt", 0o400);
echo "after chmod 0400 writable: " . (is_writable("hello.txt") ? "y" : "n") . "\n";
chmod("hello.txt", 0o644);
echo "after chmod 0644 writable: " . (is_writable("hello.txt") ? "y" : "n") . "\n";

// lchown()/lchgrp() operate on the symlink itself. Passing -1 asks libc to
// leave that ownership field unchanged, so this is a non-privileged probe.
symlink("hello.txt", "hello-link.txt");
echo "lchown symlink no-op: " . (lchown("hello-link.txt", -1) ? "y" : "n") . "\n";
echo "lchgrp symlink no-op: " . (lchgrp("hello-link.txt", -1) ? "y" : "n") . "\n";
unlink("hello-link.txt");

// umask() returns the previous mask. Save and restore it so the change
// is bounded to this example.
$old = umask(0o022);
echo "umask saved: " . sprintf("%04o", $old) . "\n";
umask($old);

unlink("hello.txt");
echo "done\n";
