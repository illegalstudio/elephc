---
title: Refresh every TLS roots constraint for the October certificate advisories
author: Guillaumeloulier
date: 2026-10-20
branch: fix/webpki-advisories-20261020
touches:
  - Cargo.lock
  - Cargo.toml
  - crates/elephc-pdo/Cargo.toml
  - crates/elephc-tls/Cargo.toml
  - ".plans/webpki-advisories-20261020.md"
---

## Decided
Upgrade every direct webpki-roots constraint (compiler monitor, TLS bridge and optional PDO TLS) to at least 1.0.10, and lock rustls-webpki 0.103.16. The October 19/20 certificate advisories affect main as well as Reflection PR1953. A fresh audit reports zero vulnerabilities after the bounded update; all three consumers check warning-free, with PDO TLS enabled.

## Tried and ruled out
Upgrading only the transitive 1.x roots would keep the affected 0.26 compatibility wrapper below the advisory patched range. Remove its three direct constraints instead. Do not suppress RUSTSEC-2026-0381 or RUSTSEC-2026-0382, or turn off certificate verification. The exact failing CI recorded three vulnerable package entries and the patched lock collapses roots to one version.

## Watch out
Previously green CI can predate a newly published advisory database. Re-check the exact new head. This change validates dependency resolution and consumer compilation, not a full native/TLS handshake target matrix; CI supplies the supported-target coverage. Preserve existing allowed warning policy unchanged.
