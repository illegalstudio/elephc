# Refresh distrusted TLS certificate dependencies

- [x] Confirm exact-head CI failures and upstream RustSec advisories.
- [x] Locate all direct webpki-roots constraints and verify the dependency graph.
- [x] Upgrade roots and certificate verifier without ignoring advisories.
- [x] Check affected compiler/TLS/PDO consumers and the fresh advisory database.
- [ ] Publish the main-based dependency fix, then refresh affected PRs.

Base: 40c2d2ab3cf96a54e73b9691c8e83075b7eefc72. All work stays in Elephc.

RUSTSEC-2026-0381 requires webpki-roots >=1.0.10; the compatibility 0.26
wrapper remains affected, even if its 1.x dependency is upgraded.
RUSTSEC-2026-0382 requires rustls-webpki >=0.103.16 in the stable 0.103 line.
The independent failure appears in issue1352 PR1953. No advisory suppression
or certificate-validation bypass is part of this fix.

Validation: `cargo check --locked -p elephc -p elephc-tls -p elephc-pdo --features elephc-pdo/tls` passes without warnings. A freshly fetched RustSec database reports zero vulnerabilities; the eight previously allowed warnings remain. No runtime/ABI source or advisory policy changed. Full target coverage is left to CI.
