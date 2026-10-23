<?php
// The ext/filter surface elephc carries: filter_var() with FILTER_VALIDATE_IP.
// Transcribed into src/filter_prelude.rs with synthetic_class::transcribe; this file
// is the oracle the transcription test compares the built AST against.
//
// The reference is php-src's ext/filter/logical_filters.c on the PHP 8.5 branch
// (8.5.13-dev, the tree the compliance suite is vendored from):
// - a VALIDATE filter returns the validated VALUE, not true — `filter_var("1.2.3.4",
//   FILTER_VALIDATE_IP)` is the string, and false on failure;
// - the family comes from the CONTENT (a ':' means IPv6, else a '.' means IPv4,
//   else the value fails); the FILTER_FLAG_IPV4 / FILTER_FLAG_IPV6 options only
//   gate that family afterwards;
// - an unknown filter id warns "filter_var(): Unknown filter with ID N" and answers
//   false;
// - IPv4 (php's _php_filter_validate_ipv4): four decimal octets 0-255, at most
//   three digits, digits only, and a leading zero only for the octet "0" itself
//   ("01.2.3.4" is invalid);
// - the RFC 6890 status table (ipv4_get_status_flags): reserved = 0/8, 127/8,
//   169.254/16, 240/4; private = 10/8, 172.16/12, 192.168/16; global =
//   192.88.99.0/24; anything else is "no special block" and skips the flag checks
//   entirely (multicast 224/4, TEST-NET 192.0.2.0/24, shared 100.64.0.0/10 pass);
// - IPv6 (php's _php_filter_validate_ipv6): full RFC 4291 grammar including one
//   "::" compression and an embedded IPv4 tail, groups 1-4 hex digits
//   case-insensitively, at most 8 groups (a compression replaces at least one),
//   length ≤ 45, and the bare ":" / ":::" are not valid compressions;
// - AN EMBEDDED IPv4 TAIL FORCES THE MAPPED FORM: php's fixup overwrites the whole
//   prefix with ::ffff:0:0/96 plus the tail's four octets, discarding the parsed
//   leading groups ("1::1.2.3.4" IS "::ffff:1.2.3.4", which NO_RES_RANGE rejects);
// - the RFC 6890 status table (ipv6_get_status_flags), on the eight 16-bit groups:
//   reserved = ::/128, ::1, ::ffff:0:0/96, fe80::/10; private = fc00::/7; global =
//   64:ff9b::/16 (RFC 6052); anything else skips the flag checks (100::/64,
//   2001:db8::/32, 2002::/16 pass — and so does the IPv4-compatible ::/96, which
//   php's table does not list);
// - flags: FILTER_FLAG_GLOBAL_RANGE rejects an in-table non-global address,
//   FILTER_FLAG_NO_PRIV_RANGE rejects a private one, FILTER_FLAG_NO_RES_RANGE a
//   reserved one;
// - non-strings (int, null, array, object, resource) fail IPv4 and IPv6 alike and
//   answer false.

function __elephc_filter_ip_v4_part_ok(string $part): bool {
    $len = strlen($part);
    if ($len === 0 || $len > 3) {
        return false;
    }
    if ($len > 1 && $part[0] === "0") {
        return false;
    }
    for ($i = 0; $i < $len; $i++) {
        $c = ord($part[$i]);
        if ($c < 48 || $c > 57) {
            return false;
        }
    }
    return (int) $part <= 255;
}

function __elephc_filter_ip_v4_valid(string $ip): bool {
    $parts = explode(".", $ip);
    if (count($parts) !== 4) {
        return false;
    }
    foreach ($parts as $part) {
        if (!__elephc_filter_ip_v4_part_ok($part)) {
            return false;
        }
    }
    return true;
}

/// RFC 6890 status of an in-table IPv4 address: 1 reserved, 2 private, 3 global,
/// 0 no special block (the caller then skips every range flag).
function __elephc_filter_ip_v4_status(string $ip): int {
    $parts = explode(".", $ip);
    $a = (int) $parts[0];
    $b = (int) $parts[1];
    $c = (int) $parts[2];
    if ($a === 0) {
        return 1;
    }
    if ($a === 10) {
        return 2;
    }
    if ($a === 100 && $b >= 64 && $b <= 127) {
        return 0;
    }
    if ($a === 127) {
        return 1;
    }
    if ($a === 169 && $b === 254) {
        return 1;
    }
    if ($a === 172 && $b >= 16 && $b <= 31) {
        return 2;
    }
    if ($a === 192 && $b === 0 && $c === 0) {
        return 0;
    }
    if ($a === 192 && $b === 88 && $c === 99) {
        return 3;
    }
    if ($a === 192 && $b === 168) {
        return 2;
    }
    if ($a === 198 && $b >= 18 && $b <= 19) {
        return 0;
    }
    if ($a === 198 && $b === 51 && $c === 100) {
        return 0;
    }
    if ($a === 203 && $b === 0 && $c === 113) {
        return 0;
    }
    if ($a >= 240) {
        return 1;
    }
    return 0;
}

function __elephc_filter_ip_hex_value(string $group): int {
    $len = strlen($group);
    if ($len === 0 || $len > 4) {
        return -1;
    }
    $value = 0;
    for ($i = 0; $i < $len; $i++) {
        $c = ord($group[$i]);
        if ($c >= 48 && $c <= 57) {
            $d = $c - 48;
        } elseif ($c >= 97 && $c <= 102) {
            $d = $c - 87;
        } elseif ($c >= 65 && $c <= 70) {
            $d = $c - 55;
        } else {
            return -1;
        }
        $value = $value * 16 + $d;
    }
    return $value;
}

function __elephc_filter_ip_v6_groups(string $ip): mixed {
    // The parsed groups as a packed array of eight 16-bit ints, or false when $ip
    // is not a syntactically valid IPv6 address at all.
    if (strlen($ip) === 0 || strlen($ip) > 45) {
        return false;
    }
    // `-v4-` is THIS parser's private placeholder for a verified IPv4 tail. An address that
    // spells it literally is not an IPv6 address, and accepting it let
    // `1:2:3:4:5:6:7:8:-v4-` pass as valid.
    if (strpos($ip, "-v4-") !== false) {
        return false;
    }
    $hasV4 = false;
    $rest = $ip;
    $lastColon = strrpos($rest, ":");
    if ($lastColon !== false) {
        $tail = substr($rest, $lastColon + 1);
        if (substr_count($tail, ".") === 3) {
            if (!__elephc_filter_ip_v4_valid($tail)) {
                return false;
            }
            $hasV4 = true;
            // The tail becomes one placeholder segment; the group accounting below
            // treats it as the two final groups of the address.
            $rest = substr($rest, 0, $lastColon + 1) . "-v4-";
        }
    }
    $segs = explode(":", $rest);
    $count = count($segs);
    // The run of empty segments left by explode() around the "::" compression.
    // One run at most: a second one means the address has two "::"s.
    $runStart = -1;
    $runLen = 0;
    $runClosed = false;
    $hexGroups = 0;
    for ($i = 0; $i < $count; $i++) {
        $seg = $segs[$i];
        if ($seg === "") {
            if ($runClosed) {
                return false;
            }
            if ($runStart === -1) {
                $runStart = $i;
                $runLen = 1;
            } else {
                $runLen = $runLen + 1;
            }
        } else {
            if ($seg !== "-v4-") {
                if (__elephc_filter_ip_hex_value($seg) < 0) {
                    return false;
                }
                $hexGroups = $hexGroups + 1;
            }
            if ($runStart !== -1) {
                $runClosed = true;
            }
        }
    }
    $total = $hexGroups + ($hasV4 ? 2 : 0);
    if ($runStart === -1) {
        if ($total !== 8) {
            return false;
        }
    } elseif ($total > 7) {
        return false;
    } elseif ($runLen >= 2) {
        // The bare "::" is three empty segments and nothing else; a leading or
        // trailing run otherwise sits at one end of the address. A long run in the
        // middle is ":::" territory and invalid.
        if ($runStart === 0 && $runStart + $runLen === $count) {
            if ($runLen !== 3) {
                return false;
            }
        } elseif ($runStart !== 0 && $runStart + $runLen !== $count) {
            return false;
        }
    } elseif ($runStart === 0 || $runStart + $runLen === $count) {
        // A single empty segment at either end is a stray colon, not "::".
        return false;
    }
    if ($hasV4) {
        // php-src's fixup: an embedded IPv4 tail replaces the whole prefix with the
        // IPv4-mapped form, discarding the parsed leading groups — so "1::1.2.3.4"
        // IS "::ffff:1.2.3.4" as far as the range flags are concerned.
        $tail = substr($ip, strrpos($ip, ":") + 1);
        $parts = explode(".", $tail);
        $bytes = [];
        for ($i = 0; $i < 10; $i++) {
            array_push($bytes, 0);
        }
        array_push($bytes, 255);
        array_push($bytes, 255);
        array_push($bytes, (int) $parts[0]);
        array_push($bytes, (int) $parts[1]);
        array_push($bytes, (int) $parts[2]);
        array_push($bytes, (int) $parts[3]);
        return $bytes;
    }
    // Expand: the compression run as a whole becomes 8 - total zero groups.
    $groups = [];
    $inRun = false;
    for ($i = 0; $i < $count; $i++) {
        $seg = $segs[$i];
        if ($seg === "") {
            if (!$inRun) {
                $inRun = true;
                $zeros = 8 - $total;
                for ($z = 0; $z < $zeros; $z++) {
                    array_push($groups, 0);
                }
            }
        } else {
            array_push($groups, __elephc_filter_ip_hex_value($seg));
        }
    }
    $bytes = [];
    foreach ($groups as $g) {
        array_push($bytes, (int) ((int) $g / 256));
        array_push($bytes, (int) $g % 256);
    }
    return $bytes;
}

/// RFC 6890 status of an in-table IPv6 address (16 bytes in): 1 reserved, 2
/// private, 3 global, 0 no special block (the caller then skips every range flag).
/// The parameter is untyped because the caller has already checked `is_array` and
/// the prelude's checker cannot narrow a `mixed` return into an `array` hint.
function __elephc_filter_ip_v6_status($bytes): int {
    $allZero = true;
    for ($i = 0; $i < 16; $i++) {
        if ($bytes[$i] !== 0) {
            $allZero = false;
        }
    }
    if ($allZero) {
        return 1;
    }
    // ::1 — fifteen zero bytes then 1.
    $loopback = true;
    for ($i = 0; $i < 15; $i++) {
        if ($bytes[$i] !== 0) {
            $loopback = false;
        }
    }
    if ($loopback) {
        return 1;
    }
    // 64:ff9b::/16, RFC 6052 — global, and checked before the mapped form like
    // php-src's else-if chain does.
    if ($bytes[0] === 0 && $bytes[1] === 0x64 && $bytes[2] === 0xff && $bytes[3] === 0x9b) {
        return 3;
    }
    // ::ffff:0:0/96, the IPv4-mapped form.
    $mapped = $bytes[10] === 0xff && $bytes[11] === 0xff;
    if ($mapped) {
        for ($i = 0; $i < 10; $i++) {
            if ($bytes[$i] !== 0) {
                $mapped = false;
            }
        }
        if ($mapped) {
            return 1;
        }
    }
    // fe80::/10 link-scoped.
    if ($bytes[0] === 0xfe && ($bytes[1] & 192) === 128) {
        return 1;
    }
    // fc00::/7 unique-local.
    if ($bytes[0] === 0xfc || $bytes[0] === 0xfd) {
        return 2;
    }
    return 0;
}

function filter_var($value, $filter = 516, $options = 0) {
    // Only FILTER_VALIDATE_IP is carried. Any other identifier — including
    // FILTER_DEFAULT — gets PHP's unknown-filter warning and false, like an id
    // nothing defines.
    if ($filter !== 275) {
        trigger_error("filter_var(): Unknown filter with ID " . (string) $filter, 512);
        return false;
    }
    if (is_array($value) || is_object($value) || is_resource($value)) {
        return false;
    }
    $ip = (string) $value;
    $flags = $options;
    // The family comes from the content, php-src style: a ':' anywhere means IPv6,
    // otherwise a '.' means IPv4, otherwise nothing validates.
    $isV6 = strpos($ip, ":") !== false;
    // An IPv4-mapped IPv6 address (`::ffff:192.0.2.1`) has a dotted tail but IS an IPv6
    // address: a colon anywhere decides IPv6, and only a colon-free dotted string is IPv4.
    $isV4 = !$isV6 && strpos($ip, ".") !== false;
    if (!$isV6 && !$isV4) {
        return false;
    }
    if (($flags & 1048576) !== 0 && ($flags & 2097152) === 0 && $isV6) {
        return false;
    }
    if (($flags & 2097152) !== 0 && ($flags & 1048576) === 0 && $isV4) {
        return false;
    }
    if ($isV6) {
        $bytes = __elephc_filter_ip_v6_groups($ip);
        if (!is_array($bytes)) {
            return false;
        }
        $status = __elephc_filter_ip_v6_status($bytes);
    } else {
        if (!__elephc_filter_ip_v4_valid($ip)) {
            return false;
        }
        $status = __elephc_filter_ip_v4_status($ip);
    }
    // An out-of-table address is "no special block": every range flag passes.
    if ($status !== 0) {
        if (($flags & 536870912) !== 0 && $status !== 3) {
            return false;
        }
        if (($flags & 8388608) !== 0 && $status === 2) {
            return false;
        }
        if (($flags & 4194304) !== 0 && $status === 1) {
            return false;
        }
    }
    return $ip;
}
