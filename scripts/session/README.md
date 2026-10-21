# Pinned session PHPT replay

The normative source is php-src `php-8.5.11` commit
`678778973bb4d4185c06ef1320de50f1a975d3c8`. Use a freshly extracted source
archive for `--php-src`, without generated PHPT scripts or outputs. Build its
PHP CLI with session support separately. The runner verifies source hashes and
the oracle's exact patch version; it inventories all **263** session PHPTs.

```sh
python3 scripts/session/test_replay.py
python3 scripts/session/replay.py \
  --php-src .artifacts/session-compliance/source-oracle \
  --php .artifacts/session-compliance/php-8.5.11/sapi/cli/php \
  --elephc target/debug/elephc \
  --output .artifacts/session-compliance/replay-final
```

Prepare the current compiler and web/Magician archives first. Freeze a copy of
the executable and archives before replaying while Cargo work is active: the
runner checks start/end artifact fingerprints and records `artifacts_stable`. The manifest pins
their hashes, the runner, changed tracked and untracked sources, every PHPT,
source revision and original INI settings. Original FILE bytes and sibling
fixtures remain unchanged. Results and complete phase output are saved per case;
`results.jsonl` is flushed after each case. Use a new output directory each time.

The reference executes through PHP CLI and must match upstream EXPECT/EXPECTF
before native compilation. The native binary receives one HTTP request and the
whole prefork process group is reaped afterward. Server diagnostics and HTTP
headers/status are retained. A matching body with unexpected diagnostics is a
failure. The matcher uses the pinned `run-tests.php` EXPECTF implementation.

This adapter currently leaves COOKIE/GET/POST_RAW/EXPECTHEADERS/CGI as explicit
adapter errors, rather than pretending to validate them. A CLI-vs-web SAPI
difference can also remain a mismatch. SKIPIF uses the oracle; CLEAN is explicitly
marked as reference-PHP cleanup. Tests/fixtures requiring a PHP subprocess are
blocked, with an executable sentinel preventing native fallback to oracle PHP.
Other exceptions and timeouts remain distinct statuses.

Filtered runs record `complete=false` and the unselected remainder. Complete
inventory coverage does not imply all tests passed. The supported PHP-profile,
SAPI, reference-identity, diagnostic and target gates in
`.plans/session-php-src-compliance.md` remain necessary for full acceptance.

The checked-in `baseline-8.5.11.json` records the first bounded correction slice:
242 syntax/feature compile blockers, 20 explicit adapter blockers and 1 upstream
XFAIL across all 263 originals. It records zero native PHPT executions. The separate
HTTP regressions and reference/heap probes are useful evidence for those fixes;
they do not turn this corpus baseline into a compliance percentage.
