# PHP DOM coverage bootstrap

`bootstrap-input.json` names the checked-in authorities: the PHP source lock,
the generated 603-operation opcode manifest, and the three frozen upstream
PHPT ledgers.  Regenerate the two derived JSON artifacts with:

```bash
python3 tools/php-dom/generate_coverage_bootstrap.py \
  --repo-root . \
  --input tests/php_dom/coverage/bootstrap-input.json \
  --manifest tests/php_dom/coverage/bootstrap-manifest.json \
  --gaps tests/php_dom/coverage/bootstrap-gaps.json
```

The manifest deliberately contains no coverage owner, Rust-test mapping,
passed-PHPT claim, build attestation, or target report.  `bootstrap-gaps.json`
is the exhaustive machine-readable backlog: 603 requirement holes, 603 route
holes, 1,056 pending PHPTs, and the three missing target attestations.

`bootstrap-strict.stderr` is the checked strict-gate rejection of that exact
manifest.  It is expected to remain non-empty until the ledgers are closed and
real reviewed evidence is supplied; `expected-fail-closed-diagnostics.txt`
also names the essential failure classes for quick review.
