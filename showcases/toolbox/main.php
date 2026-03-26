<?php
// Toolbox — a collection of small CLI utilities
// Usage: elephc showcases/toolbox/main.php && ./showcases/toolbox/main

require_once 'password.php';
require_once 'hash.php';
require_once 'convert.php';
require_once 'text.php';

echo "============================\n";
echo "     TOOLBOX (elephc)       \n";
echo "============================\n";

$running = true;
while ($running) {
    echo "\n";
    echo "[1] Password generator\n";
    echo "[2] Hash a string (MD5/SHA1)\n";
    echo "[3] Base64 encode/decode\n";
    echo "[4] Text stats\n";
    echo "[5] Number base converter\n";
    echo "[6] ROT13 cipher\n";
    echo "[q] Quit\n";
    $choice = trim(readline("> "));

    if ($choice === "1") {
        $len_input = trim(readline("Length [16]: "));
        $mode = trim(readline("Mode (1=alphanumeric, 2=all chars, 3=digits) [1]: "));
        $count_input = trim(readline("How many [5]: "));
        cmd_password($len_input, $mode, $count_input);
    } elseif ($choice === "2") {
        $input = trim(readline("String to hash: "));
        cmd_hash($input);
    } elseif ($choice === "3") {
        $mode = trim(readline("(e)ncode or (d)ecode? [e]: "));
        $input = trim(readline("Input: "));
        cmd_base64($mode, $input);
    } elseif ($choice === "4") {
        echo "Enter text:\n";
        $input = readline("");
        cmd_text_stats($input);
    } elseif ($choice === "5") {
        $input = trim(readline("Number (prefix 0x for hex, 0b for binary): "));
        cmd_convert($input);
    } elseif ($choice === "6") {
        $input = trim(readline("Text: "));
        cmd_rot13($input);
    } elseif ($choice === "q" || $choice === "Q") {
        $running = false;
    } else {
        echo "Unknown option.\n";
    }
}

echo "Bye!\n";
