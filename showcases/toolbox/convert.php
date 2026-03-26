<?php
// Number base converter and Base64

function cmd_base64($mode, $input) {
    if (strlen($input) === 0) {
        echo "Empty input.\n";
        return;
    }

    if ($mode === "d" || $mode === "D") {
        echo "  Decoded: " . base64_decode($input) . "\n";
    } else {
        echo "  Encoded: " . base64_encode($input) . "\n";
    }
}

function cmd_convert($input) {
    if (strlen($input) === 0) {
        echo "Empty input.\n";
        return;
    }

    $value = 0;
    if (str_starts_with($input, "0x") || str_starts_with($input, "0X")) {
        $hex = substr($input, 2);
        $value = parse_hex($hex);
        echo "  Input:   hex " . $input . "\n";
    } elseif (str_starts_with($input, "0b") || str_starts_with($input, "0B")) {
        $bin = substr($input, 2);
        $value = parse_bin($bin);
        echo "  Input:   bin " . $input . "\n";
    } else {
        $value = intval($input);
        echo "  Input:   dec " . $input . "\n";
    }

    echo "  Decimal: " . $value . "\n";
    echo "  Hex:     0x" . to_hex($value) . "\n";
    echo "  Binary:  0b" . to_bin($value) . "\n";
    echo "  Octal:   0" . to_oct($value) . "\n";
}

function parse_hex($s) {
    $result = 0;
    for ($i = 0; $i < strlen($s); $i++) {
        $ch = strtolower(substr($s, $i, 1));
        $code = ord($ch);
        $digit = 0;
        if ($code >= ord("0") && $code <= ord("9")) {
            $digit = $code - ord("0");
        } elseif ($code >= ord("a") && $code <= ord("f")) {
            $digit = $code - ord("a") + 10;
        }
        $result = $result * 16 + $digit;
    }
    return $result;
}

function parse_bin($s) {
    $result = 0;
    for ($i = 0; $i < strlen($s); $i++) {
        $ch = substr($s, $i, 1);
        $result = $result * 2;
        if ($ch === "1") {
            $result = $result + 1;
        }
    }
    return $result;
}

function to_hex($n) {
    if ($n === 0) {
        return "0";
    }
    $hex_chars = "0123456789abcdef";
    $result = "";
    $val = $n;
    while ($val > 0) {
        $remainder = $val % 16;
        $result = substr($hex_chars, $remainder, 1) . $result;
        $val = intdiv($val, 16);
    }
    return $result;
}

function to_bin($n) {
    if ($n === 0) {
        return "0";
    }
    $result = "";
    $val = $n;
    while ($val > 0) {
        $result = (string)($val % 2) . $result;
        $val = intdiv($val, 2);
    }
    return $result;
}

function to_oct($n) {
    if ($n === 0) {
        return "0";
    }
    $result = "";
    $val = $n;
    while ($val > 0) {
        $result = (string)($val % 8) . $result;
        $val = intdiv($val, 8);
    }
    return $result;
}
