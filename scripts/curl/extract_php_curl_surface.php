#!/usr/bin/env php
<?php
/**
 * Freeze the PHP-visible ext/curl surface (functions, classes, constants)
 * across every locally available PHP 8.2-8.5 binary, and derive libcurl
 * `curl_setopt` option kinds from a downloaded libcurl `curl.h` / `multi.h`.
 *
 * Writes scripts/docs/curl_surface.json (see Task 1 of
 * .superpowers/sdd/php-curl-family/task-1-brief.md and the plan's
 * global-constraints.md for the normative surface this is audited against).
 *
 * Usage:
 *   php scripts/curl/extract_php_curl_surface.php \
 *     [--php=/path/to/php ...]  (repeatable; default: auto-probe PATH)
 *     [--curl-header=/path/to/curl-X.Y.Z/include/curl/curl.h]
 *     [--multi-header=/path/to/curl-X.Y.Z/include/curl/multi.h]
 *     [--out=scripts/docs/curl_surface.json]  (default: stdout)
 *
 * The curl.h / multi.h headers are NOT vendored into this repo (curl is a
 * managed native dependency fetched/built by Task 2, not a source import).
 * Re-download the pinned curl tarball (see $PINS below) to a scratch
 * directory and pass its extracted include/curl/{curl,multi}.h paths when
 * re-running this script.
 *
 * CI never runs this script. Its output is committed
 * (scripts/docs/curl_surface.json) so downstream tasks (constant tables,
 * the elephc-curl bridge, option dispatch) work without a PHP or libcurl
 * source tree on disk. Re-run manually when:
 *   - the pinned libcurl or OpenSSL version changes (update $PINS below,
 *     with a byte size and SHA-256 taken from an actual downloaded file --
 *     never invent a checksum), or
 *   - a new local PHP minor version becomes available (extends real
 *     coverage beyond the hand-maintained PHP 8.5 fallback below), or
 *   - the plan's normative PHP surface (global-constraints.md) changes.
 */

declare(strict_types=1);

// ---------------------------------------------------------------------
// Pinned native versions (Task 1 locked decision). These fields are the
// audit source for Task 2's catalog entries. Values MUST be verified
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
// PHP 8.5-only additions to the normative surface (global-constraints.md
// PHP surface section + Task 1 brief). No PHP 8.5 binary is available on
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

// The exact PHP program from Task 1's brief (Step 1), run once per probed
// binary via `php -r`. Kept byte-for-byte close to the brief so the
// extraction is auditable against it.
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

/** @return array{0: string[], 1: ?string, 2: ?string, 3: ?string} [phpBins, curlHeader, multiHeader, out] */
function parse_args(array $argv): array
{
    $phpBins = [];
    $curlHeader = null;
    $multiHeader = null;
    $out = null;
    foreach (array_slice($argv, 1) as $arg) {
        if (str_starts_with($arg, '--php=')) {
            $phpBins[] = substr($arg, strlen('--php='));
        } elseif (str_starts_with($arg, '--curl-header=')) {
            $curlHeader = substr($arg, strlen('--curl-header='));
        } elseif (str_starts_with($arg, '--multi-header=')) {
            $multiHeader = substr($arg, strlen('--multi-header='));
        } elseif (str_starts_with($arg, '--out=')) {
            $out = substr($arg, strlen('--out='));
        } else {
            fwrite(STDERR, "unrecognized argument: $arg\n");
            exit(1);
        }
    }
    return [$phpBins, $curlHeader, $multiHeader, $out];
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
    preg_match_all('/^#define (CURLOPT_[A-Z0-9_]+) (CURLOPT_[A-Z0-9_]+)$/m', $headerSource, $matches, PREG_SET_ORDER);
    foreach ($matches as $m) {
        [, $old, $new] = $m;
        if (!isset($table[$old]) && isset($table[$new])) {
            $table[$old] = $table[$new];
        }
    }
    return $table;
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
    [$explicitPhpBins, $curlHeaderPath, $multiHeaderPath, $outPath] = parse_args($argv);

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
        $curlMOptTable = parse_curlopt_table(file_get_contents($multiHeaderPath), 'CURLMOPT_[A-Z0-9_]+');
    }
    $headerTable = $curlOptTable + $curlMOptTable;

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
            continue;
        }
        if (in_array($name, PHP_LAYER_OPTIONS, true)) {
            $optionKinds[$name] = 'php_layer';
            continue;
        }
        if (isset($headerTable[$name])) {
            $expected = TYPE_BASE[$headerTable[$name]['type']] + $headerTable[$name]['value'];
            if ($expected !== $value) {
                $valueCrossCheckMismatches[$name] = ['php' => $value, 'curl_h' => $expected];
            }
            $classified = kind_for($name, $headerTable[$name]['type']);
            $optionKinds[$name] = $classified['kind'];
            if ($classified['note'] !== null) {
                $optionKindNotes[$name] = $classified['note'];
            }
        } elseif ($headerTable !== []) {
            // Header was supplied but doesn't mention this PHP-visible
            // option name -- either a PHP-only pseudo-option (expected for
            // the PHP_LAYER_OPTIONS names, already handled above) or a
            // genuine gap worth surfacing instead of guessing.
            $unclassifiedOptions[] = $name;
        }
    }
    ksort($optionKinds);
    ksort($optionKindNotes);

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
            'notes' =>
                'functions/classes/constants come from a live probe of every locally ' .
                'available PHP 8.2-8.5 binary (missing minors are skipped per Task 1\'s ' .
                'brief, not fabricated). Constant numeric VALUES for CURLOPT_*/CURLMOPT_* ' .
                'are the PHP-extracted values: cross-checked programmatically against the ' .
                'pinned curl.h/multi.h CURLOPT(name,type,num) arithmetic for every name ' .
                'present in both (libcurl guarantees option numbers are permanent once ' .
                'assigned -- see valueCrossCheckMismatches, expected empty). option_kinds ' .
                'is derived exclusively from the pinned curl.h/multi.h CURLOPTTYPE_* tags, ' .
                'then the global-constraints.md php_layer override list is applied on top; ' .
                'see OBJECTPOINT_KIND_OVERRIDES / CURLSHOPT_KINDS in this script for the ' .
                'small number of hand-classified exceptions curl.h\'s type tags cannot ' .
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
