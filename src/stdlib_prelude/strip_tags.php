<?php
function __elephc_strip_tags_allowed($allowed) {
    $list = "";
    if (is_array($allowed)) {
        foreach ($allowed as $t) {
            $list .= "<" . strtolower((string) $t) . ">";
        }
    } elseif ($allowed !== null) {
        $list = strtolower((string) $allowed);
    }
    return $list;
}
function __elephc_strip_tags_norm(string $tag) {
    $n = strlen($tag);
    $name = "";
    $i = 1;
    if ($i < $n && $tag[$i] === "/") { $i++; }
    while ($i < $n) {
        $c = $tag[$i];
        if ($c === ">" || $c === "/" || $c === " " || $c === "\t" || $c === "\n" || $c === "\r" || $c === "\f" || $c === "\v") { break; }
        $name .= $c;
        $i++;
    }
    return "<" . strtolower($name) . ">";
}
function strip_tags(string $string, $allowed_tags = null) {
    $string = str_replace("\0", "", $string);
    $allowed = __elephc_strip_tags_allowed($allowed_tags);
    $hasAllowed = $allowed !== "";
    $n = strlen($string);
    $out = "";
    $i = 0;
    while ($i < $n) {
        $c = $string[$i];
        if ($c !== "<") {
            $out .= $c;
            $i++;
            continue;
        }
        $next = $i + 1 < $n ? $string[$i + 1] : "";
        if (($next === " " || $next === "\t" || $next === "\n" || $next === "\r" || $next === "\f" || $next === "\v")) {
            $out .= "<";
            $i++;
            continue;
        }
        if ($next === "?") {
            $pe = strpos($string, "?>", $i + 2);
            if ($pe === false) { break; }
            $i = $pe + 2;
            continue;
        }
        if ($next === "!" && substr($string, $i, 4) === "<!--") {
            $ce = strpos($string, "-->", $i + 2);
            if ($ce === false) { break; }
            $i = $ce + 3;
            continue;
        }
        $j = $i + 1;
        $q = "";
        $depth = 0;
        $closed = false;
        $tag = "<";
        while ($j < $n) {
            $d = $string[$j];
            if ($q !== "") {
                if ($d === $q) { $q = ""; $tag .= $d; }
                elseif ($d !== "<" && $d !== ">") { $tag .= $d; }
            } elseif ($d === "\"" || $d === "'") {
                $q = $d;
                $tag .= $d;
            } elseif ($d === "<") {
                $depth++;
            } elseif ($d === ">") {
                if ($depth > 0) {
                    $depth--;
                } else {
                    $closed = true;
                    $tag .= $d;
                    break;
                }
            } else {
                $tag .= $d;
            }
            $j++;
        }
        if (!$closed) { break; }
        if ($hasAllowed && $next !== "!" && strpos($allowed, __elephc_strip_tags_norm($tag)) !== false) {
            $out .= $tag;
        }
        $i = $j + 1;
    }
    return $out;
}
