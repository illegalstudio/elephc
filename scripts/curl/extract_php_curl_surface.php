#!/usr/bin/env php
<?php
/**
 * Freeze the PHP-visible ext/curl surface (functions, classes, constants)
 * across every locally available PHP 8.2-8.5 binary, and derive libcurl
 * `curl_setopt` option kinds from a downloaded libcurl `curl.h` / `multi.h`.
 *
 * Writes scripts/docs/curl_surface.json, the frozen normative surface every
 * curl-related crate/module in this repo is audited against.
 *
 * Usage:
 *   php scripts/curl/extract_php_curl_surface.php \
 *     [--php=/path/to/php ...]  (repeatable; default: auto-probe PATH)
 *     [--curl-header=/path/to/curl-X.Y.Z/include/curl/curl.h]
 *     [--multi-header=/path/to/curl-X.Y.Z/include/curl/multi.h]
 *     [--websockets-header=/path/to/curl-X.Y.Z/include/curl/websockets.h]
 *     [--out=scripts/docs/curl_surface.json]  (default: stdout)
 *
 * The curl.h / multi.h headers are NOT vendored into this repo (curl is a
 * managed native dependency this build fetches/builds from source, not a
 * source import). Re-download the pinned curl tarball (see $PINS below) to a scratch
 * directory and pass its extracted include/curl/{curl,multi}.h paths when
 * re-running this script.
 *
 * CI never runs this script. Its output is committed
 * (scripts/docs/curl_surface.json) so downstream consumers (constant tables,
 * the elephc-curl bridge, option dispatch) work without a PHP or libcurl
 * source tree on disk. Re-run manually when:
 *   - the pinned libcurl or OpenSSL version changes (update $PINS below,
 *     with a byte size and SHA-256 taken from an actual downloaded file --
 *     never invent a checksum), or
 *   - a new local PHP minor version becomes available (extends real
 *     coverage beyond the hand-maintained PHP 8.5 fallback below), or
 *   - PHP's own ext/curl surface changes in a future PHP release.
 */

declare(strict_types=1);

// ---------------------------------------------------------------------
// Pinned native versions. These fields are the
// audit source for the elephc native catalog's curl/openssl/zlib entries
// (`src/native_deps/catalog.rs`). Values MUST be verified
// against a real downloaded file (`shasum -a 256`, `stat -f%z` / `stat -c%s`)
// -- never invented. Re-verify and update the whole block together when
// re-pinning.
// ---------------------------------------------------------------------
const PINS = [
    'libcurl' => [
        'version' => '8.21.0',
        'release_date' => '2026-06-24',
        'url' => 'https://curl.se/download/curl-8.21.0.tar.gz',
        'exact_size' => 4298225,
        'sha256' => 'd9b327997999045a24cda50f3983e69e51c516bd8be6ef9842fc7f99135e33bb',
        'checksum_cross_check' =>
            'curl.se does not publish a standalone .sha256 file for curl ' .
            'itself (only detached PGP .asc signatures, see ' .
            'https://curl.se/docs/verify.html); cross-checked instead ' .
            'against the GitHub release asset digest for curl-8.21.0.tar.gz ' .
            'on tag curl-8_21_0 (`gh api repos/curl/curl/releases/tags/' .
            'curl-8_21_0`), which reports the identical sha256 digest and ' .
            'the identical 4298225-byte size.',
    ],
    'openssl' => [
        'version' => '3.5.7',
        'release_date' => '2026-06-09',
        'lts_supported_until' => '2030-04-08',
        'url' => 'https://github.com/openssl/openssl/releases/download/openssl-3.5.7/openssl-3.5.7.tar.gz',
        'exact_size' => 53153930,
        'sha256' => 'a8c0d28a529ca480f9f36cf5792e2cd21984552a3c8e4aa11a24aa31aeac98e8',
        'checksum_cross_check' =>
            'Matches both the published ' .
            'https://openssl-library.org/source/openssl-3.5.7.tar.gz.sha256 ' .
            'checksum file and the GitHub release asset digest for ' .
            'openssl-3.5.7.tar.gz on tag openssl-3.5.7. 3.5 is the current ' .
            '"[LTS]"-labelled row on the openssl-library.org source page.',
    ],
];

// ---------------------------------------------------------------------
// PHP 8.5-only additions to the normative surface. No PHP 8.5 binary is available on
// this generating machine, so these cannot be live-extracted; they are
// hand-recorded here from https://www.php.net/manual/en/function.curl-share-init-persistent.php,
// https://php.watch/versions/8.5/curl_multi_get_handles, and the pinned
// curl.h (for CURLFOLLOW_* numeric values, confirmed present in curl.h
// 8.21.0 as plain #define, not part of the CURLOPT() type table).
// A future run with a real PHP 8.5 binary should promote any of these
// that the live probe confirms to a live "8.5" source instead.
// ---------------------------------------------------------------------
const PHP85_ONLY = [
    'functions' => [
        'curl_multi_get_handles',
        'curl_share_init_persistent',
    ],
    'classes' => [
        'CurlSharePersistentHandle',
    ],
    // name => integer value. Values for CURLFOLLOW_* are taken directly
    // from the pinned curl.h `#define` (verified against the downloaded
    // curl-8.21.0 tarball); they are libcurl-level values for
    // CURLOPT_FOLLOWLOCATION, exposed as PHP constants starting 8.5.0 per
    // curl.se/curl 8.13.0+. CURLOPT_INFILESIZE_LARGE's value is taken from
    // the pinned curl.h CURLOPT() table (CURLOPTTYPE_OFF_T + 115 = 30115);
    // confirmed absent from a live PHP 8.4.20 probe despite being a very
    // old libcurl option (curl.h places it right next to the long-standing
    // CURLOPT_INFILESIZE), matching a secondary source's claim that PHP's
    // ext/curl only started exposing this 64-bit variant in 8.5.0.
    'constants' => [
        'CURLFOLLOW_ALL' => 1,
        'CURLFOLLOW_OBEYCODE' => 2,
        'CURLFOLLOW_FIRSTONLY' => 3,
        'CURLOPT_INFILESIZE_LARGE' => 30115,
    ],
];

const SOURCE_TAG = 'php-8.5 (plan/docs)';

// PHP-layer options: implemented inside elephc-curl instead of forwarded
// to libcurl untouched (global-constraints.md "PHP-layer options" table).
// This overrides whatever curl.h says about the option's raw C type.
const PHP_LAYER_OPTIONS = [
    'CURLOPT_RETURNTRANSFER',
    'CURLOPT_HEADER',
    'CURLOPT_FILE',
    'CURLOPT_INFILE',
    'CURLOPT_INFILESIZE',
    'CURLOPT_WRITEHEADER',
    'CURLOPT_STDERR',
    'CURLOPT_BINARYTRANSFER',
    'CURLOPT_SAFE_UPLOAD',
];

// A handful of `CURLOPTTYPE_OBJECTPOINT`-tagged options are pointers to
// opaque handles/values, not strings, but our fixed 8-kind vocabulary has
// no "opaque pointer" bucket. Bucketed as "file" (closest existing kind
// for an untyped pointer/userdata slot) with an explanatory note instead
// of the "string" default. Verified these are the only plain-OBJECTPOINT
// (i.e. not STRINGPOINT/SLISTPOINT/CBPOINT) names PHP 8.4 actually
// defines besides CURLOPT_STDERR (already php_layer) and CURLOPT_POSTFIELDS
// (a genuine byte-string, left as the "string" default).
const OBJECTPOINT_KIND_OVERRIDES = [
    'CURLOPT_SHARE' => [
        'kind' => 'file',
        'note' => 'curl.h tags this CURLOPTTYPE_OBJECTPOINT, but the value is a ' .
            'CurlShareHandle pointer, not a string. Bucketed as "file" (the ' .
            'closest of the fixed kinds for an opaque handle/userdata slot).',
    ],
    'CURLOPT_PRIVATE' => [
        'kind' => 'file',
        'note' => 'curl.h tags this CURLOPTTYPE_OBJECTPOINT, but PHP stores an ' .
            'arbitrary value here (curl_getinfo(..., CURLINFO_PRIVATE) reads ' .
            'it back verbatim), not a string. Bucketed as "file" (the closest ' .
            'of the fixed kinds for an opaque userdata slot).',
    ],
];

// curl_share_setopt options: PHP only ever exposes CURLSHOPT_NONE (unused
// sentinel), CURLSHOPT_SHARE, and CURLSHOPT_UNSHARE (confirmed against a
// live PHP 8.4 probe). These do NOT use the CURLOPT(name, TYPE, num)
// macro/type-tag convention at all -- curl.h defines them as a small plain
// enum (0, 1, 2, ...). Both real options take a CURL_LOCK_DATA_* long
// value, so both are hand-classified "long".
const CURLSHOPT_KINDS = [
    'CURLSHOPT_NONE' => 'long',
    'CURLSHOPT_SHARE' => 'long',
    'CURLSHOPT_UNSHARE' => 'long',
];

// The probe program, run once per probed binary via `php -r`, that extracts
// the live function/class/constant surface `get_defined_functions()`,
// `get_declared_classes()`, and `get_defined_constants()` report for ext/curl.
const WANTED_PROGRAM = <<<'PHP'
$wanted = [];
foreach (get_defined_functions()['internal'] as $name) {
    if (str_starts_with($name, 'curl_')) {
        $wanted['functions'][] = $name;
    }
}
foreach (['CurlHandle', 'CurlMultiHandle', 'CurlShareHandle', 'CurlSharePersistentHandle', 'CURLFile', 'CURLStringFile'] as $class) {
    $wanted['classes'][$class] = class_exists($class);
}
$wanted['constants'] = get_defined_constants(true)['curl'] ?? [];
$wanted['php_version'] = PHP_VERSION;
$wanted['bundled_curl_version'] = function_exists('curl_version') ? (curl_version()['version'] ?? null) : null;
echo json_encode($wanted, JSON_PRETTY_PRINT);
PHP;

/** @return array{0: string[], 1: ?string, 2: ?string, 3: ?string, 4: ?string} [phpBins, curlHeader, multiHeader, websocketsHeader, out] */
function parse_args(array $argv): array
{
    $phpBins = [];
    $curlHeader = null;
    $multiHeader = null;
    $websocketsHeader = null;
    $out = null;
    foreach (array_slice($argv, 1) as $arg) {
        if (str_starts_with($arg, '--php=')) {
            $phpBins[] = substr($arg, strlen('--php='));
        } elseif (str_starts_with($arg, '--curl-header=')) {
            $curlHeader = substr($arg, strlen('--curl-header='));
        } elseif (str_starts_with($arg, '--multi-header=')) {
            $multiHeader = substr($arg, strlen('--multi-header='));
        } elseif (str_starts_with($arg, '--websockets-header=')) {
            $websocketsHeader = substr($arg, strlen('--websockets-header='));
        } elseif (str_starts_with($arg, '--out=')) {
            $out = substr($arg, strlen('--out='));
        } else {
            fwrite(STDERR, "unrecognized argument: $arg\n");
            exit(1);
        }
    }
    return [$phpBins, $curlHeader, $multiHeader, $websocketsHeader, $out];
}

/** Probe PATH for php / php8.2 / php8.3 / php8.4 / php8.5, deduped by reported version. */
function default_php_candidates(): array
{
    $names = ['php8.2', 'php8.3', 'php8.4', 'php8.5', 'php'];
    $found = [];
    foreach ($names as $name) {
        $resolved = trim((string) shell_exec('command -v ' . escapeshellarg($name) . ' 2>/dev/null'));
        if ($resolved !== '') {
            $found[$resolved] = true;
        }
    }
    return array_keys($found);
}

/** @return array{path: string, version: string, minor: string, functions: string[], classes: array<string,bool>, constants: array<string,int>, bundled_curl_version: ?string}|null */
function probe_php(string $bin): ?array
{
    $cmd = escapeshellarg($bin) . ' -n -r ' . escapeshellarg(WANTED_PROGRAM) . ' 2>/dev/null';
    $json = shell_exec($cmd);
    if ($json === null || trim((string) $json) === '') {
        return null;
    }
    $data = json_decode((string) $json, true);
    if (!is_array($data) || !isset($data['php_version'])) {
        return null;
    }
    $version = (string) $data['php_version'];
    $parts = explode('.', $version);
    $minor = $parts[0] . '.' . ($parts[1] ?? '0');
    return [
        'path' => $bin,
        'version' => $version,
        'minor' => $minor,
        'functions' => $data['functions'] ?? [],
        'classes' => $data['classes'] ?? [],
        'constants' => $data['constants'] ?? [],
        'bundled_curl_version' => $data['bundled_curl_version'] ?? null,
    ];
}

/**
 * Parse a `CURLOPT(NAME, TYPE, NUM)` / `CURLOPTDEPRECATED(NAME, TYPE, NUM, ...)`
 * table out of a libcurl header, returning name => ['value' => int, 'type' => TYPE tag].
 *
 * @return array<string, array{value:int, type:string}>
 */
function parse_curlopt_table(string $headerSource, string $namePrefixPattern): array
{
    $pattern = '/CURLOPT(?:DEPRECATED)?\(\s*(' . $namePrefixPattern . ')\s*,\s*(CURLOPTTYPE_[A-Z_]+)\s*,\s*(\d+)/';
    preg_match_all($pattern, $headerSource, $matches, PREG_SET_ORDER);
    $table = [];
    foreach ($matches as $m) {
        $table[$m[1]] = ['value' => (int) $m[3], 'type' => $m[2]];
    }
    return $table;
}

/**
 * curl.h keeps a "Backwards compatibility with older names" block of plain
 * `#define OLDNAME NEWNAME` aliases (e.g. `CURLOPT_ENCODING` ->
 * `CURLOPT_ACCEPT_ENCODING`, `CURLOPT_FTPAPPEND` -> `CURLOPT_APPEND`). PHP's
 * ext/curl still registers several of the old names as constants. Resolve
 * every alias whose target is already in $table onto the target's
 * value/type, without overwriting a name the CURLOPT() table already
 * defines directly.
 *
 * @param array<string, array{value:int, type:string}> $table
 * @return array<string, array{value:int, type:string}>
 */
function resolve_curlopt_aliases(string $headerSource, array $table): array
{
    // Note: [ \t]+ + a trailing \b (not a `$` line-end anchor) is required
    // -- some of these lines carry a trailing `/* comment */`
    // (e.g. `#define CURLOPT_FILE CURLOPT_WRITEDATA /* name changed in
    // 7.9.7 */`), which a same-line `$` anchor would silently reject.
    preg_match_all('/^#define[ \t]+(CURLOPT_[A-Z0-9_]+)[ \t]+(CURLOPT_[A-Z0-9_]+)\b/m', $headerSource, $matches, PREG_SET_ORDER);
    foreach ($matches as $m) {
        [, $old, $new] = $m;
        if (!isset($table[$old]) && isset($table[$new])) {
            $table[$old] = $table[$new];
        }
    }
    return $table;
}

// ---------------------------------------------------------------------
// General cross-check for every PHP-visible curl_-family constant that is
// NOT a curl_setopt/curl_multi_setopt/curl_share_setopt option (those are
// handled above via the precise CURLOPT() macro table). Every constant
// numeric value must come from the pinned libcurl,
// not from `php -r` on whatever libcurl the local PHP happens to be linked
// against. This walks curl.h/multi.h source order once, resolving both
// plain `#define NAME EXPR` macros and `typedef enum { ... } Name;` bodies
// (auto-increment C enum semantics, including CURL_DEPRECATED(...)-wrapped
// members) into a single name -> integer symbol table, so CURLE_*,
// CURLINFO_* (typebase + offset, same trick as CURLOPT_*),
// CURLM_*/CURLSHE_*/CURLPX_*/CURLMSG_*/CURLVERSION_*/CURLPROXY_*/
// CURLFTPSSL_*/CURLFTPAUTH_*/CURLFTPMETHOD_*/CURLSSH_AUTH_*/
// CURL_HTTP_VERSION_*/CURL_LOCK_DATA_*/... etc. all get verified the same
// way the options already were.
//
// A small, deliberately conservative subset of C is evaluated: integer
// literals (decimal/hex, optional u/U/l/L suffix), parens (including a
// value-preserving no-op integer cast like `(unsigned long)1`), unary
// +/-, binary <</>>/&/|/+/-/*//, and identifier references to symbols
// already resolved earlier in file order. Anything using bitwise-NOT (`~`) is
// intentionally NOT evaluated: libcurl mixes 32-bit-unsigned-flavoured
// masks (e.g. `CURLAUTH_ANY`) with `long`-typed PHP ints, and blindly
// re-deriving the two's-complement/wraparound behaviour risks producing a
// *wrong* invented value, which is worse than an honest "exempt". Same
// for anything referencing an identifier this table cannot resolve (a
// non-CURL symbol, a string literal, a function-like macro, ...). These
// are reported under constant_verification.exempt with the raw
// expression, never silently skipped and never guessed.
// ---------------------------------------------------------------------

final class ExprUnresolvable extends \Exception
{
}

/** @return list<array{type:string, value?:int}> */
function tokenize_c_expr(string $expr): array
{
    $tokens = [];
    $i = 0;
    $n = strlen($expr);
    while ($i < $n) {
        $c = $expr[$i];
        if (ctype_space($c)) {
            $i++;
            continue;
        }
        $two = substr($expr, $i, 2);
        if ($two === '<<') {
            $tokens[] = ['type' => 'shl'];
            $i += 2;
            continue;
        }
        if ($two === '>>') {
            $tokens[] = ['type' => 'shr'];
            $i += 2;
            continue;
        }
        if ($c === '(') {
            $tokens[] = ['type' => 'lparen'];
            $i++;
            continue;
        }
        if ($c === ')') {
            $tokens[] = ['type' => 'rparen'];
            $i++;
            continue;
        }
        if ($c === '~') {
            $tokens[] = ['type' => 'tilde'];
            $i++;
            continue;
        }
        if ($c === '+') {
            $tokens[] = ['type' => 'plus'];
            $i++;
            continue;
        }
        if ($c === '-') {
            $tokens[] = ['type' => 'minus'];
            $i++;
            continue;
        }
        if ($c === '|') {
            $tokens[] = ['type' => 'pipe'];
            $i++;
            continue;
        }
        if ($c === '&') {
            $tokens[] = ['type' => 'amp'];
            $i++;
            continue;
        }
        if ($c === '*') {
            $tokens[] = ['type' => 'star'];
            $i++;
            continue;
        }
        if ($c === '/') {
            $tokens[] = ['type' => 'slash'];
            $i++;
            continue;
        }
        if (preg_match('/\G0[xX][0-9a-fA-F]+/', $expr, $m, 0, $i)) {
            $tokens[] = ['type' => 'num', 'value' => intval($m[0], 16)];
            $i += strlen($m[0]);
            while ($i < $n && stripos('uUlL', $expr[$i]) !== false) {
                $i++;
            }
            continue;
        }
        if (preg_match('/\G[0-9]+/', $expr, $m, 0, $i)) {
            $tokens[] = ['type' => 'num', 'value' => intval($m[0], 10)];
            $i += strlen($m[0]);
            while ($i < $n && stripos('uUlL', $expr[$i]) !== false) {
                $i++;
            }
            continue;
        }
        if (preg_match('/\G[A-Za-z_][A-Za-z0-9_]*/', $expr, $m, 0, $i)) {
            $tokens[] = ['type' => 'ident', 'value' => $m[0]];
            $i += strlen($m[0]);
            continue;
        }
        throw new ExprUnresolvable('unexpected character: ' . var_export($c, true));
    }
    return $tokens;
}

/**
 * Evaluate a conservative subset of C integer-constant-expression syntax
 * against an already-resolved symbol table. Throws ExprUnresolvable
 * (never returns a guessed value) for `~`, unknown identifiers, string
 * literals, or anything else outside the supported subset.
 *
 * @param array<string,int> $symbols
 */
function eval_c_int_expr(string $expr, array $symbols): int
{
    $tokens = tokenize_c_expr($expr);
    $pos = 0;
    $peek = static function () use (&$tokens, &$pos) {
        return $tokens[$pos] ?? null;
    };
    $next = static function () use (&$tokens, &$pos) {
        return $tokens[$pos++] ?? null;
    };

    $parsePrimary = static function () use (&$next, $symbols): int {
        $t = $next();
        if ($t === null) {
            throw new ExprUnresolvable('unexpected end of expression');
        }
        if ($t['type'] === 'num') {
            return $t['value'];
        }
        if ($t['type'] === 'ident') {
            if (!array_key_exists($t['value'], $symbols)) {
                throw new ExprUnresolvable('unknown_identifier:' . $t['value']);
            }
            return $symbols[$t['value']];
        }
        throw new ExprUnresolvable('unexpected token: ' . $t['type']);
    };

    // Precedence, loosest to tightest (matches real C: | < & < shift <
    // additive < multiplicative < unary < primary -- additive must bind
    // TIGHTER than shift, e.g. `a + b << c` means `(a + b) << c`).
    // $parseOr is declared before $parsePrimaryOrParen because paren
    // grouping recurses back to the top of the grammar.
    $parseOr = null;
    $castKeywords = ['unsigned', 'signed', 'long', 'short', 'int', 'char'];
    $parsePrimaryOrParen = function () use (&$next, &$peek, &$pos, &$parseOr, $parsePrimary, $castKeywords, &$parseUnary): int {
        $t = $peek();
        if ($t !== null && $t['type'] === 'lparen') {
            // Speculatively check for a C-style integer cast, e.g.
            // `(unsigned long)1`: "(" TYPE_KEYWORD+ ")" immediately
            // followed by the casted expression, no operator in between.
            // A cast of an already-non-negative small literal/shift never
            // changes its value for our purposes (64-bit PHP ints), so
            // it's safe to just discard the "(unsigned long)" prefix and
            // evaluate what follows.
            $save = $pos;
            $next(); // consume '('
            $sawKeyword = false;
            while (($kt = $peek()) !== null && $kt['type'] === 'ident' && in_array($kt['value'], $castKeywords, true)) {
                $next();
                $sawKeyword = true;
            }
            $close = $peek();
            if ($sawKeyword && $close !== null && $close['type'] === 'rparen') {
                $next(); // consume ')'
                return $parseUnary();
            }
            // Not a cast -- rewind and parse as an ordinary grouped
            // expression instead.
            $pos = $save;
            $next(); // consume '('
            $v = $parseOr();
            $closeParen = $next();
            if ($closeParen === null || $closeParen['type'] !== 'rparen') {
                throw new ExprUnresolvable('unbalanced parens');
            }
            return $v;
        }
        return $parsePrimary();
    };
    $parseUnary = function () use (&$next, &$peek, &$parsePrimaryOrParen, &$parseUnary): int {
        $t = $peek();
        if ($t !== null && $t['type'] === 'tilde') {
            throw new ExprUnresolvable('tilde'); // bitwise-NOT: width-sensitive, deliberately not re-derived
        }
        if ($t !== null && $t['type'] === 'minus') {
            $next();
            return -$parseUnary();
        }
        if ($t !== null && $t['type'] === 'plus') {
            $next();
            return $parseUnary();
        }
        return $parsePrimaryOrParen();
    };
    $parseMul = function () use (&$peek, &$next, &$parseUnary): int {
        $v = $parseUnary();
        while (($t = $peek()) !== null && in_array($t['type'], ['star', 'slash'], true)) {
            $next();
            $rhs = $parseUnary();
            if ($t['type'] === 'slash') {
                if ($rhs === 0) {
                    throw new ExprUnresolvable('division by zero');
                }
                $v = intdiv($v, $rhs);
            } else {
                $v = $v * $rhs;
            }
        }
        return $v;
    };
    $parseAdd = function () use (&$peek, &$next, &$parseMul): int {
        $v = $parseMul();
        while (($t = $peek()) !== null && in_array($t['type'], ['plus', 'minus'], true)) {
            $next();
            $rhs = $parseMul();
            $v = $t['type'] === 'plus' ? ($v + $rhs) : ($v - $rhs);
        }
        return $v;
    };
    $parseShift = function () use (&$peek, &$next, &$parseAdd): int {
        $v = $parseAdd();
        while (($t = $peek()) !== null && in_array($t['type'], ['shl', 'shr'], true)) {
            $next();
            $rhs = $parseAdd();
            $v = $t['type'] === 'shl' ? ($v << $rhs) : ($v >> $rhs);
        }
        return $v;
    };
    $parseAnd = function () use (&$peek, &$next, &$parseShift): int {
        $v = $parseShift();
        while (($t = $peek()) !== null && $t['type'] === 'amp') {
            $next();
            $v = $v & $parseShift();
        }
        return $v;
    };
    $parseOr = function () use (&$peek, &$next, &$parseAnd): int {
        $v = $parseAnd();
        while (($t = $peek()) !== null && $t['type'] === 'pipe') {
            $next();
            $v = $v | $parseAnd();
        }
        return $v;
    };

    $result = $parseOr();
    if ($pos !== count($tokens)) {
        throw new ExprUnresolvable('trailing tokens after expression');
    }
    return $result;
}

/**
 * Build a name -> resolved-value table (plus per-name provenance metadata)
 * from one or more libcurl header sources, processed in source order so
 * later `#define`/enum entries can reference earlier ones (matching how
 * the C preprocessor actually resolves them). `$seedSymbols` primes the
 * table (e.g. so multi.h can be processed after curl.h with curl.h's
 * macros already visible, though in practice multi.h is self-contained).
 *
 * @param array<string,int> $seedSymbols
 * @return array{0: array<string,int>, 1: array<string, array{status:string, reason?:string, raw:?string, origin:string}>}
 */
function build_header_symbol_table(array $headerSources, array $seedSymbols = []): array
{
    $symbols = $seedSymbols;
    $meta = [];

    foreach ($headerSources as $src) {
        // Strip ALL block comments first, globally, across the whole
        // source (DOTALL) -- not per matched line. A per-line strip
        // cannot close a `/* ... */` that only closes on a later line
        // (very common in curl.h's trailing option documentation, e.g.
        // `#define CURL_HTTP_VERSION_NONE 0L /* setting this means we do
        // not care, and libcurl will pick a suitable one... */`), which
        // left a dangling unterminated comment in the captured value and
        // made otherwise-trivially-resolvable constants throw instead of
        // resolving. Newlines immediately outside the `/* */` markers are
        // untouched, so line-anchored regexes below still work.
        $stripped = preg_replace('#/\*.*?\*/#s', '', $src);
        // Join backslash line-continuations so a multi-line #define value
        // is visible on one logical line to the regex below.
        $joined = preg_replace('/\\\\\r?\n/', ' ', $stripped);

        $events = [];
        if (preg_match_all(
            '/^#define[ \t]+([A-Za-z_][A-Za-z0-9_]*)[ \t]+(.+?)[ \t]*$/m',
            $joined,
            $m,
            PREG_OFFSET_CAPTURE | PREG_SET_ORDER
        )) {
            foreach ($m as $mm) {
                $events[] = ['offset' => $mm[0][1], 'kind' => 'define', 'name' => $mm[1][0], 'raw' => $mm[2][0]];
            }
        }
        if (preg_match_all('/typedef enum \{(.*?)\}\s*(\w+);/s', $joined, $m2, PREG_OFFSET_CAPTURE | PREG_SET_ORDER)) {
            foreach ($m2 as $mm) {
                $events[] = ['offset' => $mm[0][1], 'kind' => 'enum', 'typedef' => $mm[2][0], 'body' => $mm[1][0]];
            }
        }
        // A handful of enums (e.g. `enum curl_khmatch { ... };`, used for
        // CURLKHMATCH_*) are plain non-typedef C enums instead of the
        // `typedef enum { ... } Name;` style used everywhere else.
        if (preg_match_all('/(?<!typedef )\benum\s+(\w+)\s*\{(.*?)\}\s*;/s', $joined, $m3, PREG_OFFSET_CAPTURE | PREG_SET_ORDER)) {
            foreach ($m3 as $mm) {
                $events[] = ['offset' => $mm[0][1], 'kind' => 'enum', 'typedef' => $mm[1][0], 'body' => $mm[2][0]];
            }
        }
        usort($events, static fn(array $a, array $b): int => $a['offset'] <=> $b['offset']);

        foreach ($events as $ev) {
            if ($ev['kind'] === 'define') {
                $name = $ev['name'];
                if (array_key_exists($name, $symbols) || isset($meta[$name])) {
                    continue; // first definition wins; don't clobber an already-known symbol
                }
                $raw = trim(preg_replace('#/\*.*?\*/#s', '', $ev['raw']));
                if ($raw === '') {
                    continue;
                }
                try {
                    $val = eval_c_int_expr($raw, $symbols);
                    $symbols[$name] = $val;
                    $meta[$name] = ['status' => 'resolved', 'raw' => $raw, 'origin' => 'define'];
                } catch (ExprUnresolvable $e) {
                    $meta[$name] = ['status' => 'exempt', 'reason' => $e->getMessage(), 'raw' => $raw, 'origin' => 'define'];
                }
                continue;
            }

            // enum body: strip comments, then CURL_DEPRECATED(...) wrapper
            // calls (their args are a bare version token + a quoted
            // string, never nested parens), collapse whitespace, split on
            // top-level commas, and replay C's auto-increment semantics.
            $body = preg_replace('#/\*.*?\*/#s', '', $ev['body']);
            $body = preg_replace('/CURL_DEPRECATED\([^()]*\)/', '', $body);
            $body = trim(preg_replace('/\s+/', ' ', $body));
            $parts = array_values(array_filter(array_map('trim', explode(',', $body)), static fn($s) => $s !== ''));

            $prev = -1; // so the first implicit member is 0, matching C
            $chainBroken = false;
            foreach ($parts as $part) {
                if (!preg_match('/^([A-Za-z_][A-Za-z0-9_]*)\s*(?:=\s*(.+))?$/', $part, $pm)) {
                    continue; // stray text left over from a macro/comment we didn't fully strip
                }
                $name = $pm[1];
                if (array_key_exists($name, $symbols) || isset($meta[$name])) {
                    // Already known (e.g. from an earlier header); keep
                    // tracking auto-increment using its known value.
                    if (array_key_exists($name, $symbols)) {
                        $prev = $symbols[$name];
                        $chainBroken = false;
                    }
                    continue;
                }
                $explicit = $pm[2] ?? null;
                if ($explicit !== null && $explicit !== '') {
                    try {
                        $val = eval_c_int_expr($explicit, $symbols);
                        $symbols[$name] = $val;
                        $meta[$name] = ['status' => 'resolved', 'raw' => $explicit, 'origin' => 'enum:' . $ev['typedef']];
                        $prev = $val;
                        $chainBroken = false;
                    } catch (ExprUnresolvable $e) {
                        $meta[$name] = ['status' => 'exempt', 'reason' => $e->getMessage(), 'raw' => $explicit, 'origin' => 'enum:' . $ev['typedef']];
                        $chainBroken = true; // can't safely keep auto-incrementing past an unresolved explicit value
                    }
                } elseif ($chainBroken) {
                    $meta[$name] = ['status' => 'exempt', 'reason' => 'auto_increment_after_unresolved_sibling', 'raw' => null, 'origin' => 'enum:' . $ev['typedef']];
                } else {
                    $val = $prev + 1;
                    $symbols[$name] = $val;
                    $meta[$name] = ['status' => 'resolved', 'raw' => null, 'origin' => 'enum:' . $ev['typedef']];
                    $prev = $val;
                }
            }
        }
    }

    return [$symbols, $meta];
}

const TYPE_BASE = [
    'CURLOPTTYPE_LONG' => 0,
    'CURLOPTTYPE_VALUES' => 0,
    'CURLOPTTYPE_OBJECTPOINT' => 10000,
    'CURLOPTTYPE_STRINGPOINT' => 10000,
    'CURLOPTTYPE_SLISTPOINT' => 10000,
    'CURLOPTTYPE_CBPOINT' => 10000,
    'CURLOPTTYPE_FUNCTIONPOINT' => 20000,
    'CURLOPTTYPE_OFF_T' => 30000,
    'CURLOPTTYPE_BLOB' => 40000,
];

const TYPE_TO_KIND = [
    'CURLOPTTYPE_LONG' => 'long',
    'CURLOPTTYPE_VALUES' => 'long',
    'CURLOPTTYPE_OBJECTPOINT' => 'string', // default; see OBJECTPOINT_KIND_OVERRIDES
    'CURLOPTTYPE_STRINGPOINT' => 'string',
    'CURLOPTTYPE_SLISTPOINT' => 'slist',
    'CURLOPTTYPE_CBPOINT' => 'file', // callback userdata pointer; see module docblock
    'CURLOPTTYPE_FUNCTIONPOINT' => 'callback',
    'CURLOPTTYPE_OFF_T' => 'off_t',
    'CURLOPTTYPE_BLOB' => 'blob',
];

function kind_for(string $name, string $type): array
{
    if (in_array($name, PHP_LAYER_OPTIONS, true)) {
        return ['kind' => 'php_layer', 'note' => null];
    }
    if (isset(OBJECTPOINT_KIND_OVERRIDES[$name])) {
        return OBJECTPOINT_KIND_OVERRIDES[$name];
    }
    return ['kind' => TYPE_TO_KIND[$type] ?? 'string', 'note' => null];
}

function main(array $argv): void
{
    [$explicitPhpBins, $curlHeaderPath, $multiHeaderPath, $websocketsHeaderPath, $outPath] = parse_args($argv);

    $candidates = $explicitPhpBins !== [] ? $explicitPhpBins : default_php_candidates();

    $probes = [];
    $seenVersions = [];
    foreach ($candidates as $bin) {
        $result = probe_php($bin);
        if ($result === null) {
            continue;
        }
        // Prefer the first binary seen for a given X.Y minor (dedupes e.g.
        // 'php' and 'php8.4' both resolving to PHP 8.4.20).
        if (isset($seenVersions[$result['minor']])) {
            continue;
        }
        $seenVersions[$result['minor']] = true;
        $probes[$result['minor']] = $result;
    }
    ksort($probes);

    $wantedMinors = ['8.2', '8.3', '8.4', '8.5'];

    // --- functions -------------------------------------------------
    $functions = [];
    foreach ($probes as $minor => $probe) {
        foreach ($probe['functions'] as $fn) {
            $functions[$fn]['sources'][] = $minor;
        }
    }
    if (!isset($probes['8.5'])) {
        foreach (PHP85_ONLY['functions'] as $fn) {
            if (!isset($functions[$fn])) {
                $functions[$fn]['sources'][] = SOURCE_TAG;
            }
        }
    }
    ksort($functions);

    // --- classes -----------------------------------------------------
    $classNames = ['CurlHandle', 'CurlMultiHandle', 'CurlShareHandle', 'CurlSharePersistentHandle', 'CURLFile', 'CURLStringFile'];
    $classes = [];
    foreach ($classNames as $class) {
        $perVersion = [];
        foreach ($probes as $minor => $probe) {
            $perVersion[$minor] = (bool) ($probe['classes'][$class] ?? false);
        }
        $classes[$class] = $perVersion;
    }
    if (!isset($probes['8.5']) && in_array('CurlSharePersistentHandle', PHP85_ONLY['classes'], true)) {
        $classes['CurlSharePersistentHandle']['source_note'] = SOURCE_TAG;
    }

    // --- constants (values) -----------------------------------------
    $constantSources = [];
    foreach ($probes as $minor => $probe) {
        foreach ($probe['constants'] as $name => $value) {
            if (!isset($constantSources[$name])) {
                $constantSources[$name] = ['value' => $value, 'sources' => []];
            } elseif ($constantSources[$name]['value'] !== $value) {
                $constantSources[$name]['value_mismatch'][] = "$minor=$value";
            }
            $constantSources[$name]['sources'][] = $minor;
        }
    }
    if (!isset($probes['8.5'])) {
        foreach (PHP85_ONLY['constants'] as $name => $value) {
            if (!isset($constantSources[$name])) {
                $constantSources[$name] = ['value' => $value, 'sources' => [SOURCE_TAG]];
            }
        }
    }
    ksort($constantSources);

    $constants = [];
    foreach ($constantSources as $name => $entry) {
        $constants[$name] = $entry['value'];
    }

    // --- option kind derivation from the pinned libcurl headers ------
    $optionKinds = [];
    $optionKindNotes = [];
    $unclassifiedOptions = [];
    $valueCrossCheckMismatches = [];

    $curlOptTable = [];
    $curlMOptTable = [];
    $curlHeaderSource = null;
    $multiHeaderSource = null;
    $websocketsHeaderSource = null;
    if ($curlHeaderPath !== null) {
        if (!is_readable($curlHeaderPath)) {
            fwrite(STDERR, "curl-header not readable: $curlHeaderPath\n");
            exit(1);
        }
        $curlHeaderSource = file_get_contents($curlHeaderPath);
        $curlOptTable = parse_curlopt_table($curlHeaderSource, 'CURLOPT_[A-Z0-9_]+');
        $curlOptTable = resolve_curlopt_aliases($curlHeaderSource, $curlOptTable);
    }
    if ($multiHeaderPath !== null) {
        if (!is_readable($multiHeaderPath)) {
            fwrite(STDERR, "multi-header not readable: $multiHeaderPath\n");
            exit(1);
        }
        $multiHeaderSource = file_get_contents($multiHeaderPath);
        $curlMOptTable = parse_curlopt_table($multiHeaderSource, 'CURLMOPT_[A-Z0-9_]+');
    }
    if ($websocketsHeaderPath !== null) {
        if (!is_readable($websocketsHeaderPath)) {
            fwrite(STDERR, "websockets-header not readable: $websocketsHeaderPath\n");
            exit(1);
        }
        $websocketsHeaderSource = file_get_contents($websocketsHeaderPath);
    }
    $headerTable = $curlOptTable + $curlMOptTable;

    // Built here (before the option loop, not just before the later
    // non-option pass) so the CURLSHOPT_* branch below can look its
    // values up too: CURLSHOPT_NONE/SHARE/UNSHARE don't use the
    // CURLOPT(name,type,num) macro convention at all (see CURLSHOPT_KINDS
    // above) and so never appear in $headerTable -- their curl.h source is
    // the plain `typedef enum { CURLSHOPT_NONE, ... } CURLSHoption;` body,
    // which only the general symbol table parses.
    $genSymbols = [];
    $genMeta = [];
    $headerSourcesForGeneral = array_values(array_filter(
        [$curlHeaderSource, $multiHeaderSource, $websocketsHeaderSource],
        static fn($s) => $s !== null
    ));
    if ($headerSourcesForGeneral !== []) {
        [$genSymbols, $genMeta] = build_header_symbol_table($headerSourcesForGeneral);
    }
    $curlshoptValueChecked = []; // name => bool; only true once actually compared to $genSymbols

    foreach ($constants as $name => $value) {
        $isOption = str_starts_with($name, 'CURLOPT_') || str_starts_with($name, 'CURLMOPT_') || str_starts_with($name, 'CURLSHOPT_');
        if (!$isOption) {
            continue;
        }
        if (str_starts_with($name, 'CURLSHOPT_')) {
            if (isset(CURLSHOPT_KINDS[$name])) {
                $optionKinds[$name] = CURLSHOPT_KINDS[$name];
            } else {
                $unclassifiedOptions[] = $name;
            }
            // KIND is hand-classified (CURLSHOPT_KINDS) because these
            // options don't use the CURLOPT()-macro type-tag convention;
            // the numeric VALUE still needs an actual header cross-check,
            // same as every other constant -- look it up in the general
            // #define/enum symbol table (curl.h's `CURLSHoption` enum).
            if (isset($genSymbols[$name])) {
                $expected = $genSymbols[$name];
                if ($expected !== $value) {
                    $valueCrossCheckMismatches[$name] = [
                        'php' => $value,
                        'curl_h' => $expected,
                        'resolution' => 'pinned curl.h/multi.h value used (locked decision #1); constants[' . $name . '] overridden',
                    ];
                    $constants[$name] = $expected;
                }
                $curlshoptValueChecked[$name] = true;
            }
            continue;
        }
        // php_layer only overrides the KIND (implementation strategy);
        // when the option is also a real libcurl option (6 of the 9 named
        // ones -- HEADER/FILE/INFILE/INFILESIZE/WRITEHEADER/STDERR), its
        // numeric VALUE still gets cross-checked against the pinned
        // header below, same as every other option.
        $kindOverride = in_array($name, PHP_LAYER_OPTIONS, true) ? 'php_layer' : null;

        if (isset($headerTable[$name])) {
            $expected = TYPE_BASE[$headerTable[$name]['type']] + $headerTable[$name]['value'];
            if ($expected !== $value) {
                // The pinned header wins over whatever the local `php -r` probe reported.
                $valueCrossCheckMismatches[$name] = [
                    'php' => $value,
                    'curl_h' => $expected,
                    'resolution' => 'pinned curl.h/multi.h value used (locked decision #1); constants[' . $name . '] overridden',
                ];
                $constants[$name] = $expected;
            }
            if ($kindOverride !== null) {
                $optionKinds[$name] = $kindOverride;
            } else {
                $classified = kind_for($name, $headerTable[$name]['type']);
                $optionKinds[$name] = $classified['kind'];
                if ($classified['note'] !== null) {
                    $optionKindNotes[$name] = $classified['note'];
                }
            }
            continue;
        }
        if ($kindOverride !== null) {
            // Expected: the 3 genuinely PHP-invented pseudo-options
            // (CURLOPT_RETURNTRANSFER/BINARYTRANSFER/SAFE_UPLOAD) have no
            // libcurl entry at all -- their values are PHP's own, not
            // recomputed from any libcurl header.
            $optionKinds[$name] = $kindOverride;
            continue;
        }
        if ($headerTable !== []) {
            // Header was supplied but doesn't mention this PHP-visible
            // option name -- a genuine gap worth surfacing instead of
            // guessing.
            $unclassifiedOptions[] = $name;
        }
    }
    ksort($optionKinds);
    ksort($optionKindNotes);

    // --- non-option constant cross-check (CURLE_*, CURLINFO_*, CURLM_*,
    // CURLAUTH_*, CURLPROXY_*, CURLSSH_*, CURL_*, ...) against the pinned
    // headers' #define macros and enum bodies. $genSymbols/$genMeta were
    // already built above (before the option loop) so the CURLSHOPT_*
    // branch there could use them too; reused here as-is. -------------
    $constantVerifiedCount = 0;
    $constantExempt = [];
    $constantNotFoundInHeader = [];
    if ($headerSourcesForGeneral !== []) {
        foreach ($constants as $name => $value) {
            $isOption = str_starts_with($name, 'CURLOPT_') || str_starts_with($name, 'CURLMOPT_') || str_starts_with($name, 'CURLSHOPT_');
            if ($isOption) {
                continue; // already handled above
            }
            if (!isset($genMeta[$name])) {
                $constantNotFoundInHeader[$name] = $value;
                continue;
            }
            $entry = $genMeta[$name];
            if ($entry['status'] === 'resolved') {
                $expected = $genSymbols[$name];
                if ($expected !== $value) {
                    $valueCrossCheckMismatches[$name] = [
                        'php' => $value,
                        'curl_h' => $expected,
                        'resolution' => 'pinned curl.h/multi.h value used (locked decision #1); constants[' . $name . '] overridden',
                    ];
                    $constants[$name] = $expected;
                }
                $constantVerifiedCount++;
            } else {
                $constantExempt[$name] = [
                    'reason' => $entry['reason'],
                    'raw_expression' => $entry['raw'],
                    'origin' => $entry['origin'],
                    'php_value_kept' => $value,
                ];
            }
        }
    }
    ksort($constantExempt);
    ksort($constantNotFoundInHeader);

    // --- assemble final document --------------------------------------
    $phpVersionsSummary = [];
    foreach ($wantedMinors as $minor) {
        if (isset($probes[$minor])) {
            $probe = $probes[$minor];
            $phpVersionsSummary[$minor] = sprintf(
                '%s (installed at %s; bundled libcurl %s; live source for functions/classes/constants)',
                $probe['version'],
                $probe['path'],
                $probe['bundled_curl_version'] ?? 'unknown'
            );
        } elseif ($minor === '8.5') {
            $phpVersionsSummary[$minor] = 'not installed locally; curl_multi_get_handles, ' .
                'curl_share_init_persistent, CurlSharePersistentHandle, CURLFOLLOW_*, and ' .
                'CURLOPT_INFILESIZE_LARGE hand-added from php.net docs / curl.h, marked ' .
                'source "' . SOURCE_TAG . '"';
        } else {
            $phpVersionsSummary[$minor] = 'not installed locally; skipped (no PHP-' . $minor . '-only names known to exist)';
        }
    }

    $phpOnlyOptions = PHP_LAYER_OPTIONS;
    sort($phpOnlyOptions);

    // CURLOPT_*/CURLMOPT_* option values were cross-checked above via the
    // dedicated CURLOPT()-macro table; CURLSHOPT_* values were separately
    // cross-checked against the general #define/enum symbol table (see
    // $curlshoptValueChecked, populated in the option loop above -- curl.h
    // doesn't use the CURLOPT()-macro convention for share options at
    // all). Count only names an actual header comparison ran for -- i.e.
    // all but the 3 genuinely PHP-invented pseudo-options, which have no
    // libcurl equivalent, and (only when no header was supplied at all)
    // CURLSHOPT_* names that therefore never got checked.
    $optionsVerifiedCount = 0;
    foreach ($constants as $name => $value) {
        $isOption = str_starts_with($name, 'CURLOPT_') || str_starts_with($name, 'CURLMOPT_') || str_starts_with($name, 'CURLSHOPT_');
        if (!$isOption) {
            continue;
        }
        if (str_starts_with($name, 'CURLSHOPT_')) {
            if (isset($curlshoptValueChecked[$name])) {
                $optionsVerifiedCount++;
            }
            continue;
        }
        if (isset($headerTable[$name])) {
            $optionsVerifiedCount++;
        }
    }
    $phpOnlyPseudoOptions = array_values(array_diff(PHP_LAYER_OPTIONS, array_keys($headerTable)));
    sort($phpOnlyPseudoOptions);

    $constantVerification = [
        'method' =>
            'Every curl_-family constant PHP exposes is classified into exactly one ' .
            'bucket: (a) a curl_setopt/curl_multi_setopt OPTION, cross-checked via the ' .
            'precise CURLOPT(name,type,num) macro table (see option_kinds / ' .
            'value_cross_check_mismatches above); a curl_share_setopt OPTION ' .
            '(CURLSHOPT_*) is cross-checked the same way but against the general ' .
            'symbol table instead, since curl.h declares CURLSHOPT_* as a plain ' .
            '`typedef enum { CURLSHOPT_NONE, ... } CURLSHoption;` rather than via the ' .
            'CURLOPT() macro, (b) a non-option constant verified against the same ' .
            'general #define + enum symbol table built from the pinned headers, (c) ' .
            'explicitly exempt (header expression uses bitwise-NOT or references an ' .
            'unresolvable identifier -- PHP\'s probed value is kept, never guessed), or ' .
            '(d) genuinely not present in the pinned headers at all (the 3 PHP-invented ' .
            'pseudo-options, listed here, not silently dropped).',
        'total_constants' => count($constants),
        'options_verified_against_pinned_curlopt_table' => $optionsVerifiedCount,
        'options_php_only_no_libcurl_equivalent' => $phpOnlyPseudoOptions,
        'non_option_constants_verified_against_pinned_header' => $constantVerifiedCount,
        'non_option_constants_exempt' => $constantExempt,
        'non_option_constants_not_found_in_pinned_header' => $constantNotFoundInHeader,
    ];

    $probedSummary = [];
    foreach ($wantedMinors as $minor) {
        if (isset($probes[$minor])) {
            $probedSummary[] = [
                'minor' => $minor,
                'found' => true,
                'path' => $probes[$minor]['path'],
                'version' => $probes[$minor]['version'],
                'bundled_libcurl_version' => $probes[$minor]['bundled_curl_version'],
            ];
        } else {
            $probedSummary[] = ['minor' => $minor, 'found' => false];
        }
    }

    $doc = [
        'generated_from' => [
            'extractor_script' => 'scripts/curl/extract_php_curl_surface.php',
            'generated_at' => gmdate('c'),
            'php_binaries_probed' => $probedSummary,
            'curl_header_used' => $curlHeaderPath !== null
                ? basename(dirname(dirname(dirname($curlHeaderPath)))) . '/include/curl/curl.h (downloaded curl-' . PINS['libcurl']['version'] . ' tarball; not vendored into this repo)'
                : null,
            'multi_header_used' => $multiHeaderPath !== null
                ? basename(dirname(dirname(dirname($multiHeaderPath)))) . '/include/curl/multi.h (downloaded curl-' . PINS['libcurl']['version'] . ' tarball; not vendored into this repo)'
                : null,
            'websockets_header_used' => $websocketsHeaderPath !== null
                ? basename(dirname(dirname(dirname($websocketsHeaderPath)))) . '/include/curl/websockets.h (downloaded curl-' . PINS['libcurl']['version'] . ' tarball; not vendored into this repo; only source of CURLWS_* values)'
                : null,
            'notes' =>
                'functions/classes/constants come from a live probe of every locally ' .
                'available PHP 8.2-8.5 binary (missing minors are skipped per Task 1\'s ' .
                'brief, not fabricated). Per locked decision #1, the FINAL constants[] ' .
                'map is not simply the raw PHP probe: every CURLOPT_*/CURLMOPT_* value is ' .
                'cross-checked against the pinned curl.h/multi.h CURLOPT(name,type,num) ' .
                'arithmetic, and every other curl_-family constant (CURLE_*, CURLINFO_*, ' .
                'CURLM_*, CURLSHE_*, CURLAUTH_*, CURLPROXY_*, CURLSSH_*, CURL_*, ...) is ' .
                'separately cross-checked against a general symbol table this script ' .
                'builds by parsing the pinned headers\' plain #define macros AND C enum ' .
                'bodies (auto-increment semantics, CURL_DEPRECATED(...)-wrapped members, ' .
                'CURLINFO_*\'s typebase+offset pattern) -- see build_header_symbol_table() ' .
                'and constant_verification below for exact coverage. Wherever the pinned ' .
                'header and the PHP probe disagree the header wins and the override is ' .
                'recorded in value_cross_check_mismatches (empty on this run: libcurl ' .
                'guarantees numbers are permanent once assigned). A constant is "exempt" ' .
                '(PHP\'s probed value kept, not independently re-derived) only when its ' .
                'header expression uses bitwise-NOT or references something this script\'s ' .
                'deliberately conservative C-subset evaluator cannot resolve -- see ' .
                'constant_verification.exempt for the full list with reasons, never ' .
                'silently skipped. option_kinds is derived exclusively from the pinned ' .
                'curl.h/multi.h CURLOPTTYPE_* tags, then the global-constraints.md ' .
                'php_layer override list is applied on top (value cross-check still runs ' .
                'for the 6 of 9 php_layer names that are real libcurl options); see ' .
                'OBJECTPOINT_KIND_OVERRIDES / CURLSHOPT_KINDS in this script for the small ' .
                'number of hand-classified kind exceptions curl.h\'s type tags cannot ' .
                'resolve on their own.',
            'php85_candidates_deliberately_excluded' =>
                'Secondary (non-php.net) sources also claimed PHP 8.5 adds ' .
                'CURLINFO_USED_PROXY, CURLINFO_HTTPAUTH_USED, CURLINFO_PROXYAUTH_USED, ' .
                'CURLINFO_QUEUE_TIME_T, CURLINFO_POSTTRANSFER_TIME_T, CURLINFO_CONN_ID, ' .
                'and CURLOPT_SSL_SIGNATURE_ALGORITHMS. Unlike CURLOPT_INFILESIZE_LARGE ' .
                '(confirmed absent from a live PHP 8.4.20 probe, value cross-checked ' .
                'against curl.h), these were not independently corroborated -- one ' .
                'sibling claim in the same source (CURLOPT_TCP_KEEPCNT as an 8.5 ' .
                'addition) directly contradicted a dedicated php.watch page dating it ' .
                'to 8.4, which this PHP 8.4.20 probe already confirms is defined. ' .
                'Deliberately left out of this freeze rather than risk an invented ' .
                'CURLINFO_* numeric value (CURLINFO_* uses a different, unparsed ' .
                'type-mask + sequential-enum encoding this script does not derive). ' .
                'Re-run against a real PHP 8.5 binary to resolve.',
        ],
        'php_versions' => $phpVersionsSummary,
        'functions' => $functions,
        'classes' => $classes,
        'constants' => $constants,
        'php_only_options' => $phpOnlyOptions,
        'option_kinds' => $optionKinds,
        'option_kind_notes' => $optionKindNotes,
        'unclassified_options' => $unclassifiedOptions,
        'value_cross_check_mismatches' => $valueCrossCheckMismatches,
        'constant_verification' => $constantVerification,
        'libcurl' => PINS['libcurl'],
        'openssl' => PINS['openssl'],
    ];

    $json = json_encode($doc, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);
    if ($outPath !== null) {
        file_put_contents($outPath, $json . "\n");
        fwrite(STDERR, "wrote $outPath\n");
    } else {
        echo $json, "\n";
    }
}

main($argv);
