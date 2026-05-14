<?php
// Symbolic and hard links — symlink() points to a target by name, link()
// shares the same inode, readlink() reveals the symlink target.

file_put_contents("data.txt", "original payload\n");

// Soft (symbolic) link: points by path.
symlink("data.txt", "soft.txt");
echo "via soft link: " . file_get_contents("soft.txt");

// readlink() returns the path the symlink points to.
echo "soft.txt -> " . readlink("soft.txt") . "\n";

// Hard link: a second name for the same inode.
link("data.txt", "hard.txt");
echo "via hard link: " . file_get_contents("hard.txt");

// linkinfo() returns a non-zero device id for an existing link, -1 otherwise.
echo "linkinfo(soft.txt) > 0: " . (linkinfo("soft.txt") > 0 ? "y" : "n") . "\n";
echo "linkinfo(missing): " . linkinfo("does-not-exist") . "\n";

unlink("soft.txt");
unlink("hard.txt");
unlink("data.txt");
echo "done\n";
