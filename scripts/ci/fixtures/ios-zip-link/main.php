<?php

// Exercises the bridge surface the iOS host link must resolve: create an archive,
// write an entry through libzip's writer, then read it back through the reader. The
// host only calls the exported function, but elephc_init() runs this top-level code.

$file = sys_get_temp_dir() . '/elephc-ios-zip-link.zip';
@unlink($file);

$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('smoke.txt', 'zip bridge');
$zip->close();

$zip = new ZipArchive();
$zip->open($file);
echo $zip->getFromName('smoke.txt'), "\n";
$zip->close();
@unlink($file);

#[Export]
function ios_zip_link_smoke(): int
{
    return 0;
}
