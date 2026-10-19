<?php
function __elephc_pack_int(int $v, int $size, bool $big) {
    $s = "";
    for ($i = 0; $i < $size; $i++) {
        $b = ($v >> (8 * $i)) & 255;
        if ($big) { $s = chr($b) . $s; } else { $s .= chr($b); }
    }
    return $s;
}
function __elephc_pack_float_bits(float $v, int $ebits, int $mbits) {
    $bias = (1 << ($ebits - 1)) - 1;
    $emax = (1 << $ebits) - 1;
    $sign = 0;
    if ($v < 0.0 || ($v === 0.0 && fdiv(1.0, $v) < 0.0)) { $sign = 1; $v = -$v; }
    $expField = 0;
    $mant = 0;
    if ($v !== $v) {
        $expField = $emax;
        $mant = 1 << ($mbits - 1);
        $sign = 0;
    } elseif ($v === INF) {
        $expField = $emax;
    } elseif ($v > 0.0) {
        $e = (int) floor(log($v, 2.0));
        $m = $v / pow(2.0, (float) $e);
        while ($m >= 2.0) { $m = $m / 2.0; $e++; }
        while ($m < 1.0) { $m = $m * 2.0; $e--; }
        $scale = pow(2.0, (float) $mbits);
        if ($e < 1 - $bias) {
            $x = $v / pow(2.0, (float) (1 - $bias - $mbits));
            $f = floor($x);
            $d = $x - $f;
            if ($d > 0.5 || ($d === 0.5 && fmod($f, 2.0) === 1.0)) { $f = $f + 1.0; }
            $mant = (int) $f;
            $expField = 0;
            if ($mant >= (1 << $mbits)) { $mant = 0; $expField = 1; }
        } else {
            $x = ($m - 1.0) * $scale;
            $f = floor($x);
            $d = $x - $f;
            if ($d > 0.5 || ($d === 0.5 && fmod($f, 2.0) === 1.0)) { $f = $f + 1.0; }
            $mant = (int) $f;
            if ($mant >= (1 << $mbits)) { $mant = 0; $e++; }
            $expField = $e + $bias;
            if ($expField >= $emax) { $expField = $emax; $mant = 0; }
        }
    }
    return ($sign << ($ebits + $mbits)) | ($expField << $mbits) | $mant;
}
function pack(string $format, mixed ...$values) {
    $n = strlen($format);
    $out = "";
    $ai = 0;
    $argc = count($values);
    $i = 0;
    while ($i < $n) {
        $code = $format[$i];
        $i++;
        $count = 1;
        $star = false;
        $hasCount = false;
        if ($i < $n && $format[$i] === "*") {
            $star = true;
            $hasCount = true;
            $i++;
        } else {
            $digits = "";
            while ($i < $n && (ord($format[$i]) >= 48 && ord($format[$i]) <= 57)) {
                $digits .= $format[$i];
                $i++;
            }
            if ($digits !== "") { $count = (int) $digits; $hasCount = true; }
        }
        if ($code === "a" || $code === "A" || $code === "Z") {
            if ($ai >= $argc) { throw new ValueError("Type " . $code . ": not enough arguments"); }
            $s = (string) $values[$ai];
            $ai++;
            $len = strlen($s);
            if ($star) { $count = $len; if ($code === "Z") { $count = $len + 1; } }
            $pad = $code === "A" ? " " : "\0";
            $take = $count;
            if ($code === "Z" && $count > 0) { $take = $count - 1; }
            $chunk = substr($s, 0, $take);
            $out .= $chunk;
            $fill = $count - strlen($chunk);
            if ($fill > 0) { $out .= str_repeat($pad, $fill); }
        } elseif ($code === "h" || $code === "H") {
            if ($ai >= $argc) { throw new ValueError("Type " . $code . ": not enough arguments"); }
            $s = (string) $values[$ai];
            $ai++;
            $len = strlen($s);
            if ($star) { $count = $len; }
            if ($count > $len) { $count = $len; }
            $nib = [];
            for ($k = 0; $k < $count; $k++) {
                $ch = $s[$k];
                if ((ord($ch) >= 48 && ord($ch) <= 57)) { $nib[] = ord($ch) - 48; }
                elseif ((ord($ch) >= 97 && ord($ch) <= 102)) { $nib[] = ord($ch) - 87; }
                elseif ((ord($ch) >= 65 && ord($ch) <= 70)) { $nib[] = ord($ch) - 55; }
                else { throw new ValueError("Type " . $code . ": illegal hex digit " . $ch); }
            }
            if (count($nib) % 2 === 1) { $nib[] = 0; }
            for ($k = 0; $k < count($nib); $k += 2) {
                if ($code === "H") { $out .= chr(($nib[$k] << 4) | $nib[$k + 1]); }
                else { $out .= chr(($nib[$k + 1] << 4) | $nib[$k]); }
            }
        } elseif ($code === "x") {
            if ($star) { $count = 0; }
            $out .= str_repeat("\0", $count);
        } elseif ($code === "X") {
            if ($star) { $count = 0; }
            $keep = strlen($out) - $count;
            if ($keep < 0) { $keep = 0; }
            $out = substr($out, 0, $keep);
        } elseif ($code === "@") {
            if ($star) { $count = 0; }
            $cur = strlen($out);
            if ($count > $cur) { $out .= str_repeat("\0", $count - $cur); }
            else { $out = substr($out, 0, $count); }
        } else {
            $size = 0;
            $big = false;
            $kind = 0;
            if ($code === "c" || $code === "C") { $size = 1; }
            elseif ($code === "s" || $code === "S") { $size = 2; }
            elseif ($code === "n") { $size = 2; $big = true; }
            elseif ($code === "v") { $size = 2; }
            elseif ($code === "i" || $code === "I" || $code === "l" || $code === "L") { $size = 4; }
            elseif ($code === "N") { $size = 4; $big = true; }
            elseif ($code === "V") { $size = 4; }
            elseif ($code === "q" || $code === "Q" || $code === "P") { $size = 8; }
            elseif ($code === "J") { $size = 8; $big = true; }
            elseif ($code === "f" || $code === "g") { $size = 4; $kind = 1; }
            elseif ($code === "G") { $size = 4; $big = true; $kind = 1; }
            elseif ($code === "d" || $code === "e") { $size = 8; $kind = 1; }
            elseif ($code === "E") { $size = 8; $big = true; $kind = 1; }
            else { throw new ValueError("Type " . $code . ": unknown format code"); }
            if ($star) { $count = $argc - $ai; }
            if ($ai + $count > $argc) { throw new ValueError("Type " . $code . ": too few arguments"); }
            for ($k = 0; $k < $count; $k++) {
                $v = $values[$ai];
                $ai++;
                if ($kind === 1) {
                    $bits = $size === 4 ? __elephc_pack_float_bits((float) $v, 8, 23) : __elephc_pack_float_bits((float) $v, 11, 52);
                    $out .= __elephc_pack_int($bits, $size, $big);
                } else {
                    $out .= __elephc_pack_int((int) $v, $size, $big);
                }
            }
        }
    }
    return $out;
}
