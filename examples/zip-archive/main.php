<?php

// PHP 8.6's in-memory ZipArchive: build an archive, serialize it, read it back,
// and extract it without touching a ZIP file on disk.
$zip = new ZipArchive();
$zip->openString();
$zip->addFromString("hello.txt", "Hello from elephc\n");
$zip->addEmptyDir("docs");
$zip->addFromString("docs/readme.txt", "Nested entry\n");
$zip->setArchiveComment("Built in memory");
$zip->setCompressionName("hello.txt", ZipArchive::CM_STORE);

$bytes = $zip->closeString();
if (!is_string($bytes)) {
    fwrite(STDERR, "closeString() failed\n");
    exit(1);
}
echo "archive bytes: ", strlen($bytes), "\n";

$copy = new ZipArchive();
$copy->openString($bytes);
echo $copy->numFiles, " entries\n";
echo $copy->getFromName("hello.txt");
echo "comment: ", $copy->getArchiveComment(), "\n";

$target = __DIR__ . "/out";
$copy->extractTo($target);
echo "extracted: ", file_get_contents($target . "/docs/readme.txt");
