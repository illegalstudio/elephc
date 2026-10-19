<?php
function __elephc_unpack_int(string $data, int $pos, int $size, bool $big, bool $signed) {
    $v = 0;
    for ($i = 0; $i < $size; $i++) {
        $b = ord($data[$pos + ($big ? $i : $size - 1 - $i)]);
        $v = ($v << 8) | $b;
    }
    if ($signed && $size < 8) {
        $top = 1 << (8 * $size - 1);
        if (($v & $top) !== 0) { $v = $v - ($top << 1); }
    }
    return $v;
}
function __elephc_unpack_float(int $bits, int $ebits, int $mbits) {
    $emax = (1 << $ebits) - 1;
    $bias = $emax >> 1;
    $mask = (1 << $mbits) - 1;
    $mant = $bits & $mask;
    $expField = ($bits >> $mbits) & $emax;
    $neg = (($bits >> ($ebits + $mbits)) & 1) === 1;
    if ($expField === $emax) {
        if ($mant !== 0) { return NAN; }
        return $neg ? -INF : INF;
    }
    $scale = pow(2.0, (float) $mbits);
    if ($expField === 0) {
        $val = ((float) $mant / $scale) * pow(2.0, (float) (1 - $bias));
    } else {
        $val = (1.0 + (float) $mant / $scale) * pow(2.0, (float) ($expField - $bias));
    }
    return $neg ? -$val : $val;
}
function unpack(string $format, string $string, int $offset = 0) {
    $result = [];
    $dlen = strlen($string);
    $pos = $offset;
    if ($pos < 0 || $pos > $dlen) {
        throw new ValueError('unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)');
    }
    $parts = explode("/", $format);
    foreach ($parts as $part) {
        if ($part === "") { continue; }
        $code = $part[0];
        $n = strlen($part);
        $i = 1;
        $star = false;
        $hasCount = false;
        $count = 1;
        if ($i < $n && $part[$i] === "*") {
            $star = true;
            $hasCount = true;
            $i++;
        } else {
            $digits = "";
            while ($i < $n && (ord($part[$i]) >= 48 && ord($part[$i]) <= 57)) {
                $digits .= $part[$i];
                $i++;
            }
            if ($digits !== "") { $count = (int) $digits; $hasCount = true; }
        }
        $name = substr($part, $i);
        $rest = $dlen - $pos;
        if ($code === "a" || $code === "A" || $code === "Z" || $code === "h" || $code === "H") {
            if ($code === "h" || $code === "H") {
                $nibbles = $star ? $rest * 2 : $count;
                $bytes = intdiv($nibbles + 1, 2);
            } else {
                $bytes = $star ? $rest : $count;
                $nibbles = 0;
            }
            if ($bytes > $rest) {
                return false;
            }
            $chunk = substr($string, $pos, $bytes);
            $pos += $bytes;
            if ($code === "a") {
                $val = $chunk;
            } elseif ($code === "A") {
                $val = rtrim($chunk, " \t\r\n\0");
            } elseif ($code === "Z") {
                $z = strpos($chunk, "\0");
                $val = $z === false ? $chunk : substr($chunk, 0, $z);
            } else {
                $hex = bin2hex($chunk);
                if ($code === "h") {
                    $sw = "";
                    for ($k = 0; $k + 1 < strlen($hex); $k += 2) { $sw .= $hex[$k + 1] . $hex[$k]; }
                    $hex = $sw;
                }
                $val = substr($hex, 0, $nibbles);
            }
            if ($name === "") { $result[1] = $val; } else { $result[$name] = $val; }
            continue;
        }
        if ($code === "x") {
            $pos += $star ? 0 : $count;
            continue;
        }
        if ($code === "X") {
            $pos -= $star ? 0 : $count;
            if ($pos < 0) { $pos = 0; }
            continue;
        }
        if ($code === "@") {
            $pos = $offset + ($star ? 0 : $count);
            continue;
        }
        $size = 0;
        $big = false;
        $signed = false;
        $kind = 0;
        if ($code === "c") { $size = 1; $signed = true; }
        elseif ($code === "C") { $size = 1; }
        elseif ($code === "s") { $size = 2; $signed = true; }
        elseif ($code === "S" || $code === "v") { $size = 2; }
        elseif ($code === "n") { $size = 2; $big = true; }
        elseif ($code === "i" || $code === "l") { $size = 4; $signed = true; }
        elseif ($code === "I" || $code === "L" || $code === "V") { $size = 4; }
        elseif ($code === "N") { $size = 4; $big = true; }
        elseif ($code === "q") { $size = 8; $signed = true; }
        elseif ($code === "Q" || $code === "P") { $size = 8; }
        elseif ($code === "J") { $size = 8; $big = true; }
        elseif ($code === "f" || $code === "g") { $size = 4; $kind = 1; }
        elseif ($code === "G") { $size = 4; $big = true; $kind = 1; }
        elseif ($code === "d" || $code === "e") { $size = 8; $kind = 1; }
        elseif ($code === "E") { $size = 8; $big = true; $kind = 1; }
        else { throw new ValueError("Invalid format type " . $code); }
        $reps = $star ? intdiv($dlen - $pos, $size) : $count;
        for ($k = 0; $k < $reps; $k++) {
            if ($pos + $size > $dlen) {
                return false;
            }
            if ($kind === 1) {
                $bits = __elephc_unpack_int($string, $pos, $size, $big, false);
                $fval = $size === 4 ? __elephc_unpack_float($bits, 8, 23) : __elephc_unpack_float($bits, 11, 52);
            } else {
                $ival = __elephc_unpack_int($string, $pos, $size, $big, $signed);
            }
            $pos += $size;
            if ($name === "") {
                $key = $k + 1;
            } elseif ($star || $count !== 1) {
                $key = $name . ($k + 1);
            } else {
                $key = $name;
            }
            if ($kind === 1) { $result[$key] = $fval; } else { $result[$key] = $ival; }
        }
    }
    return $result;
}
