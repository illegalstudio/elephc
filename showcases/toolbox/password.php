<?php
// Password generator

function cmd_password($len_input, $mode, $count_input) {
    $length = 16;
    if (strlen($len_input) > 0) {
        $length = intval($len_input);
    }
    if ($length < 4) {
        $length = 4;
    }
    if ($length > 128) {
        $length = 128;
    }

    if ($mode !== "2" && $mode !== "3") {
        $mode = "1";
    }

    $count = 5;
    if (strlen($count_input) > 0) {
        $count = intval($count_input);
    }
    if ($count < 1) {
        $count = 1;
    }
    if ($count > 20) {
        $count = 20;
    }

    echo "\n";
    for ($i = 0; $i < $count; $i++) {
        echo "  " . generate_password($length, $mode) . "\n";
    }
    echo "\n";
}

function generate_password($length, $mode) {
    $lower = "abcdefghijklmnopqrstuvwxyz";
    $upper = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    $digits = "0123456789";
    $special = "!@#$%^&*()-_=+[]{}|;:,.<>?";

    $charset = "";
    if ($mode === "1") {
        $charset = $lower . $upper . $digits;
    } elseif ($mode === "2") {
        $charset = $lower . $upper . $digits . $special;
    } elseif ($mode === "3") {
        $charset = $digits;
    }

    $charset_len = strlen($charset);
    $password = "";
    for ($i = 0; $i < $length; $i++) {
        $idx = random_int(0, $charset_len - 1);
        $password .= substr($charset, $idx, 1);
    }

    return $password;
}
