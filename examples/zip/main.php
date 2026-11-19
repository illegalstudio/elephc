<?php

// Create an archive with a file, a string entry, and an empty directory.
$archive = __DIR__ . '/demo.zip';
@unlink($archive);

$zip = new ZipArchive();
if ($zip->open($archive, ZipArchive::CREATE | ZipArchive::OVERWRITE) !== true) {
    exit(1);
}

$zip->addFromString('readme.txt', "Built by elephc's libzip-backed Zip bridge.\n");
$zip->addFile(__FILE__, 'source/main.php');
$zip->addEmptyDir('assets');
$zip->setCommentName('readme.txt', 'generated');
$zip->close();

// Open it again and inspect the entries.
$zip = new ZipArchive();
if ($zip->open($archive) !== true) {
    exit(1);
}

echo "entries: ", $zip->numFiles, "\n";
for ($index = 0; $index < $zip->numFiles; $index++) {
    $stat = $zip->statIndex($index);
    echo $index, ' ', $stat['name'], ' (', $stat['size'], " bytes)\n";
}

echo $zip->getFromName('readme.txt');
echo 'archive comment: ', $zip->getCommentName('readme.txt'), "\n";

// Extract everything and read one file back.
$destination = __DIR__ . '/out';
$zip->extractTo($destination);
echo 'extracted: ', file_get_contents($destination . '/readme.txt');
$zip->close();

unlink($archive);
unlink($destination . '/readme.txt');
unlink($destination . '/source/main.php');
rmdir($destination . '/source');
rmdir($destination . '/assets');
rmdir($destination);
