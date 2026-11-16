<?php
// Run `elephc native install --locked` in this directory before compiling.
$info = new finfo(FILEINFO_MIME_TYPE);
foreach (["plain text", "<html><body>Hello</body></html>", "{\"name\":\"Elephc\"}"] as $contents) {
    echo $info->buffer($contents), "\n";
}
echo $info->buffer("Hello", FILEINFO_MIME), "\n";
echo "Source MIME: ", mime_content_type(__FILE__), "\n";
finfo_close($info);
echo $info->buffer("still usable"), "\n";
