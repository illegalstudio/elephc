<?php
// Compile with --ini default_charset=8bit to count bytes in imported labels.
// The default UTF-8 configuration counts the accented letter as one character.
$label = "Café";
echo "Configured encoding: ", mb_internal_encoding(), "\n";
echo "Label length: ", mb_strlen($label), "\n";
echo "Configured language: ", ini_get("mbstring.language"), "\n";
ini_set("mbstring.language", "Japanese");
echo "Temporary language: ", mb_language(), "\n";
ini_restore("mbstring.language");
echo "Restored language: ", mb_language(), "\n";
