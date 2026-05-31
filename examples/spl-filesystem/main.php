<?php

class ExampleInfo extends SplFileInfo {}
class ExampleFile extends SplFileObject {}

mkdir("spl-fs");
mkdir("spl-fs/nested");
file_put_contents("spl-fs/readme.txt", "alpha\nbeta\n");
file_put_contents("spl-fs/nested/leaf.txt", "leaf\n");

echo "file info:\n";
$info = new SplFileInfo("spl-fs/readme.txt");
echo $info->getFilename();
echo " ";
echo $info->getExtension();
echo " ";
echo $info->getSize();
echo "\n";

echo "file factories:\n";
$info->setInfoClass(ExampleInfo::class);
$info->setFileClass(ExampleFile::class);
echo $info->getFileInfo() instanceof ExampleInfo ? "info" : "base";
echo " ";
echo $info->openFile() instanceof ExampleFile ? "file" : "base";
echo "\n";

echo "file object:\n";
$file = $info->openFile();
foreach ($file as $line => $text) {
    echo $line;
    echo ":";
    echo trim($text);
    echo "\n";
}

echo "filesystem:\n";
$paths = new FilesystemIterator(
    "spl-fs",
    FilesystemIterator::KEY_AS_FILENAME |
    FilesystemIterator::CURRENT_AS_PATHNAME |
    FilesystemIterator::SKIP_DOTS
);
foreach ($paths as $name => $path) {
    echo $name;
    echo "=";
    echo $path;
    echo "\n";
}

echo "glob:\n";
$matches = new GlobIterator(
    "spl-fs/*.txt",
    FilesystemIterator::KEY_AS_FILENAME | FilesystemIterator::CURRENT_AS_PATHNAME
);
foreach ($matches as $name => $path) {
    echo $name;
    echo "=";
    echo $path;
    echo "\n";
}

echo "recursive directory:\n";
$tree = new RecursiveDirectoryIterator(
    "spl-fs",
    FilesystemIterator::KEY_AS_FILENAME |
    FilesystemIterator::CURRENT_AS_PATHNAME |
    FilesystemIterator::SKIP_DOTS
);
foreach ($tree as $name => $path) {
    if ($tree->hasChildren()) {
        $child = $tree->getChildren();
        $child->rewind();
        echo $name;
        echo "/";
        echo $child->key();
        echo "=";
        echo $child->current();
        echo "\n";
    }
}

echo "recursive cache:\n";
$cache = new RecursiveCachingIterator(new RecursiveArrayIterator(["group" => ["leaf" => 7]]));
$cache->rewind();
if ($cache->hasChildren()) {
    $child = $cache->getChildren();
    $child->rewind();
    echo $cache->key();
    echo "/";
    echo $child->key();
    echo "=";
    echo $child->current();
    echo "\n";
}

unlink("spl-fs/nested/leaf.txt");
rmdir("spl-fs/nested");
unlink("spl-fs/readme.txt");
rmdir("spl-fs");
