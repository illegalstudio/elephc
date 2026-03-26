<?php
// Hash and ROT13 utilities

function cmd_hash($input) {
    if (strlen($input) === 0) {
        echo "Empty input.\n";
        return;
    }
    echo "  MD5:    " . md5($input) . "\n";
    echo "  SHA1:   " . sha1($input) . "\n";
    echo "  SHA256: " . hash("sha256", $input) . "\n";
}

function cmd_rot13($input) {
    if (strlen($input) === 0) {
        echo "Empty input.\n";
        return;
    }

    $result = "";
    for ($i = 0; $i < strlen($input); $i++) {
        $ch = substr($input, $i, 1);
        $code = ord($ch);
        if ($code >= 65 && $code <= 90) {
            $result .= chr(($code - 65 + 13) % 26 + 65);
        } elseif ($code >= 97 && $code <= 122) {
            $result .= chr(($code - 97 + 13) % 26 + 97);
        } else {
            $result .= $ch;
        }
    }

    echo "  ROT13: " . $result . "\n";
}
