<?php
function __elephc_version_canon(string $ver) {
    $n = strlen($ver);
    $out = $ver[0];
    $lp = $ver[0];
    for ($i = 1; $i < $n; $i++) {
        $c = $ver[$i];
        $lq = $out[strlen($out) - 1];
        $cDigit = (ord($c) >= 48 && ord($c) <= 57);
        $lpDigit = (ord($lp) >= 48 && ord($lp) <= 57);
        $lpNdig = !$lpDigit && $lp !== ".";
        $cNdig = !$cDigit && $c !== ".";
        $cAlnum = $cDigit || ((ord($c) >= 97 && ord($c) <= 122)) || ((ord($c) >= 65 && ord($c) <= 90));
        if ($c === "-" || $c === "_" || $c === "+") {
            if ($lq !== ".") { $out .= "."; }
        } elseif (($lpNdig && $cDigit) || ($lpDigit && $cNdig)) {
            if ($lq !== ".") { $out .= "."; }
            $out .= $c;
        } elseif (!$cAlnum) {
            if ($lq !== ".") { $out .= "."; }
        } else {
            $out .= $c;
        }
        $lp = $c;
    }
    if (strlen($out) > 1 && substr($out, -1) === ".") { $out = substr($out, 0, -1); }
    return $out;
}
function __elephc_version_form(string $s) {
    if (strncmp($s, "dev", 3) === 0) { return 0; }
    if (strncmp($s, "alpha", 5) === 0) { return 1; }
    if (strncmp($s, "a", 1) === 0) { return 1; }
    if (strncmp($s, "beta", 4) === 0) { return 2; }
    if (strncmp($s, "b", 1) === 0) { return 2; }
    if (strncmp($s, "RC", 2) === 0) { return 3; }
    if (strncmp($s, "rc", 2) === 0) { return 3; }
    if (strncmp($s, "#", 1) === 0) { return 4; }
    if (strncmp($s, "pl", 2) === 0) { return 5; }
    if (strncmp($s, "p", 1) === 0) { return 5; }
    return -1;
}
function __elephc_version_special(string $a, string $b) {
    $x = __elephc_version_form($a);
    $y = __elephc_version_form($b);
    return $x < $y ? -1 : ($x > $y ? 1 : 0);
}
function __elephc_version_raw(string $v1, string $v2) {
    if ($v1 === "" || $v2 === "") {
        if ($v1 === "" && $v2 === "") { return 0; }
        return $v1 !== "" ? 1 : -1;
    }
    $pa = explode(".", __elephc_version_canon($v1));
    $pb = explode(".", __elephc_version_canon($v2));
    $na = count($pa);
    $nb = count($pb);
    $i = 0;
    $j = 0;
    $more1 = true;
    $more2 = true;
    $r = 0;
    while ($more1 && $more2 && $pa[$i] !== "" && $pb[$j] !== "") {
        $s1 = $pa[$i];
        $s2 = $pb[$j];
        $more1 = $i < $na - 1;
        $more2 = $j < $nb - 1;
        $d1 = (ord($s1[0]) >= 48 && ord($s1[0]) <= 57);
        $d2 = (ord($s2[0]) >= 48 && ord($s2[0]) <= 57);
        if ($d1 && $d2) {
            $l1 = (int) $s1;
            $l2 = (int) $s2;
            $r = $l1 < $l2 ? -1 : ($l1 > $l2 ? 1 : 0);
        } elseif (!$d1 && !$d2) {
            $r = __elephc_version_special($s1, $s2);
        } elseif ($d1) {
            $r = __elephc_version_special("#N#", $s2);
        } else {
            $r = __elephc_version_special($s1, "#N#");
        }
        if ($r !== 0) { break; }
        if ($more1) { $i++; }
        if ($more2) { $j++; }
    }
    if ($r === 0) {
        if ($more1) {
            $s1 = $pa[$i];
            if ($s1 !== "" && (ord($s1[0]) >= 48 && ord($s1[0]) <= 57)) {
                $r = 1;
            } else {
                $r = __elephc_version_raw(implode(".", array_slice($pa, $i)), "#N#");
            }
        } elseif ($more2) {
            $s2 = $pb[$j];
            if ($s2 !== "" && (ord($s2[0]) >= 48 && ord($s2[0]) <= 57)) {
                $r = -1;
            } else {
                $r = __elephc_version_raw("#N#", implode(".", array_slice($pb, $j)));
            }
        }
    }
    return $r;
}
function version_compare(string $version1, string $version2, ?string $operator = null) {
    $r = __elephc_version_raw($version1, $version2);
    if ($operator === null) { return $r; }
    $op = $operator;
    if ($op === "<" || $op === "lt") { return $r < 0; }
    if ($op === "<=" || $op === "le") { return $r <= 0; }
    if ($op === ">" || $op === "gt") { return $r > 0; }
    if ($op === ">=" || $op === "ge") { return $r >= 0; }
    if ($op === "==" || $op === "=" || $op === "eq") { return $r === 0; }
    if ($op === "!=" || $op === "<>" || $op === "ne") { return $r !== 0; }
    throw new ValueError('version_compare(): Argument #3 ($operator) must be a valid comparison operator');
}
