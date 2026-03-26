<?php
// Text analysis

function cmd_text_stats($input) {
    if (strlen($input) === 0) {
        echo "Empty input.\n";
        return;
    }

    $len = strlen($input);
    $words = explode(" ", $input);
    $word_count = 0;
    foreach ($words as $w) {
        if (strlen(trim($w)) > 0) {
            $word_count++;
        }
    }

    // Count using ord() on each char without creating substrings
    $letters = 0;
    $digits = 0;
    $spaces = 0;
    $upper_count = 0;
    $lower_count = 0;
    // Use a single pass with explode to avoid substr per char
    // Actually, just count using built-in string functions
    $lower_version = strtolower($input);
    $upper_version = strtoupper($input);

    // Count spaces
    $no_spaces = str_replace(" ", "", $input);
    $spaces = $len - strlen($no_spaces);

    // Count digits by removing them
    $no_digits = $input;
    $no_digits = str_replace("0", "", $no_digits);
    $no_digits = str_replace("1", "", $no_digits);
    $no_digits = str_replace("2", "", $no_digits);
    $no_digits = str_replace("3", "", $no_digits);
    $no_digits = str_replace("4", "", $no_digits);
    $no_digits = str_replace("5", "", $no_digits);
    $no_digits = str_replace("6", "", $no_digits);
    $no_digits = str_replace("7", "", $no_digits);
    $no_digits = str_replace("8", "", $no_digits);
    $no_digits = str_replace("9", "", $no_digits);
    $digits = $len - strlen($no_digits);

    // Letters = those that change case
    // A letter is a char where lower != upper
    // Use ctype functions
    $alpha_only = "";
    $alpha_count = 0;
    // Approximate: total - spaces - digits - punctuation
    // Use ctype_alpha on the whole string isn't helpful
    // Just report what we can compute efficiently
    $letters = $len - $spaces - $digits;
    // Subtract non-letter non-digit non-space chars
    $clean = str_replace(" ", "", $input);
    $clean = str_replace("0", "", $clean);
    $clean = str_replace("1", "", $clean);
    $clean = str_replace("2", "", $clean);
    $clean = str_replace("3", "", $clean);
    $clean = str_replace("4", "", $clean);
    $clean = str_replace("5", "", $clean);
    $clean = str_replace("6", "", $clean);
    $clean = str_replace("7", "", $clean);
    $clean = str_replace("8", "", $clean);
    $clean = str_replace("9", "", $clean);
    $letters = strlen($clean);
    $other = $len - $letters - $digits - $spaces;

    echo "\n";
    echo "  Characters: " . $len . "\n";
    echo "  Words:      " . $word_count . "\n";
    echo "  Letters:    " . $letters . "\n";
    echo "  Digits:     " . $digits . "\n";
    echo "  Spaces:     " . $spaces . "\n";
    echo "  Other:      " . $other . "\n";
    echo "  Reversed:   " . strrev($input) . "\n";
    echo "  Uppercase:  " . $upper_version . "\n";
    echo "  Lowercase:  " . $lower_version . "\n";
}
