<?php
// File I/O operations

// Write a file
file_put_contents("greeting.txt", "Hello from elephc!\n");

// Read it back
$content = file_get_contents("greeting.txt");
print $content;

// Check file properties
if (file_exists("greeting.txt")) {
    echo "File exists, size: " . filesize("greeting.txt") . " bytes\n";
}
if (is_file("greeting.txt")) {
    echo "It is a regular file\n";
}
if (is_readable("greeting.txt")) {
    echo "It is readable\n";
}

// Use fopen/fwrite/fclose for line-by-line writing
$f = fopen("numbers.txt", "w");
$i = 1;
while ($i <= 5) {
    fwrite($f, "Line " . $i . "\n");
    $i++;
}
fclose($f);

// Read file into array of lines
$lines = file("numbers.txt");
echo "Lines in file: " . count($lines) . "\n";

// Read line by line with fgets
$f = fopen("numbers.txt", "r");
while (!feof($f)) {
    $line = fgets($f);
    if (strlen($line) > 0) {
        echo "  " . trim($line) . "\n";
    }
}
fclose($f);

// Directory operations
mkdir("testdir");
if (is_dir("testdir")) {
    echo "Created directory\n";
}
rmdir("testdir");

// Copy and rename
copy("greeting.txt", "backup.txt");
rename("backup.txt", "archive.txt");
echo "Copied and renamed: " . file_get_contents("archive.txt");

// Var dump for debugging
var_dump(42);
var_dump("hello");
var_dump(true);
var_dump(3.14);

// Current working directory
$cwd = getcwd();
echo "Working in: " . $cwd . "\n";

// Cleanup
unlink("greeting.txt");
unlink("numbers.txt");
unlink("archive.txt");
echo "Done!\n";
