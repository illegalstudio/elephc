# PDO parity: remaining registry and qualification work

- [x] Freeze the PHP 8.2-8.5 reference matrix from versioned php-src/PECL sources.
- [x] Ship the compiled-driver registry and runtime `pdo.dsn.*` alias resolution.
- [x] Implement all eleven driver surfaces in the frozen matrix, including versioned constants and subclasses.
- [ ] Complete migration of driver attributes, subclasses, statements and capability hooks to the registry boundary.
- [ ] Resolve unintended common PDO/PDOStatement divergences and document deliberate compatibility decisions.
- [ ] Complete cross-target live qualification and proprietary-client fixtures.
- [ ] Refresh the source/version audit and documentation when each remaining gap closes.

## Current baseline and qualification limits

Reviewed against release candidate `df57a9800f` on 2026-10-19. The maintained
[PDO guide](../docs/php/pdo.md#limitations) records eleven implemented drivers,
optional-profile build coverage on macOS ARM64, Linux ARM64 and Linux x86_64,
and Linux live acceptance for PostgreSQL, MySQL, DBLIB, Firebird, ODBC, SQLSRV,
OCI and CUBRID. SQLite uses the ordinary codegen suite. This cleanup inspected
sources and workflows; it did not rerun that historical qualification.

Informix and IBM still require proprietary Client SDK/server fixtures for live
acceptance. Cross-target live execution and the documented common divergences
remain open. A compiled surface or a Linux service run does not establish full
parity on every target. iOS device and Simulator remain first-class compiler
targets; any client/profile restrictions need explicit diagnostics and documented
qualification boundaries rather than an implied waiver of that policy.

| Responsibility | Source of truth |
|---|---|
| Driver availability, names and DSN dispatch | `crates/elephc-pdo/src/driver.rs` |
| Runtime alias configuration and precedence | `crates/elephc-pdo/src/ini.rs` |
| Connections, statements and native client adapters | `crates/elephc-pdo/src/` |
| PHP-facing attributes, subclasses and methods | `src/pdo_prelude.rs`, `src/pdo_prelude/` |
| Existing live acceptance workflow | `.github/workflows/pdo-live.yml` |

The former delivery order and future-tense alias proposal are complete. Runtime
aliases load the explicit `PHPRC` configuration and sorted `PHP_INI_SCAN_DIR`
fragments; later assignments win. Preserve undefined-alias versus unavailable-
driver errors, credential precedence, persistence keys and `uri:` handling.

## Normative scope

The following matrix freezes PHP 8.2-8.5 compatibility decisions as of
2026-07-16. It is not a claim about upstream branch support on the review date.
Refresh the versioned sources explicitly before expanding this baseline.
External driver releases define their own contract alongside generic PDO.

## Frozen upstream matrix (2026-07-16)

| Driver | PHP 8.2 | PHP 8.3 | PHP 8.4 | PHP 8.5 | Native client/reference |
| --- | --- | --- | --- | --- | --- |
| mysql | php-src | php-src | php-src | php-src | mysqlnd behavior; Rust wire client must match it |
| pgsql | php-src | php-src | php-src | php-src | libpq behavior; `libpq-gss` for GSS paths |
| sqlite | php-src | php-src | php-src | php-src | SQLite C API |
| dblib | php-src legacy class | php-src legacy class | `Pdo\Dblib` + aliases | class + deprecated aliases | FreeTDS DB-Library (`libsybdb`) |
| firebird | php-src legacy class | php-src legacy class | `Pdo\Firebird` + aliases | class + deprecated aliases | Firebird client (`fbclient`) |
| odbc | php-src | php-src | php-src | php-src | unixODBC/iODBC + selected ODBC driver |
| oci | bundled php-src | bundled php-src / PECL transition | PECL | PECL | Oracle Instant Client OCI |
| sqlsrv | Microsoft 5.12 line | Microsoft 5.12 line | Microsoft 5.12+ | Microsoft 5.13+ | Microsoft ODBC Driver 17/18 |
| ibm | PECL | PECL | `Pdo\Ibm` in PECL 1.7 | class + deprecated aliases | IBM CLI/ODBC |
| informix | PECL | PECL | PECL | PECL | Informix CSDK CLI |
| cubrid | external CUBRID extension | external CUBRID extension | external CUBRID extension | external CUBRID extension | CUBRID CCI |

Normative external sources are the maintained upstream repositories/releases:

- PDO_OCI: <https://github.com/php/pecl-database-pdo_oci>
- PDO_SQLSRV: <https://github.com/microsoft/msphpsql>
- PDO_IBM: <https://github.com/php/pecl-database-pdo_ibm>
- PDO_INFORMIX: <https://pecl.php.net/package/PDO_INFORMIX>
- PDO_CUBRID: <https://github.com/CUBRID/cubrid-pdo>

The word “legacy class” means the driver is usable through `PDO`, while its namespaced
`Pdo\<Driver>` subclass has not yet been introduced by that PHP/extension version. Old
driver constants remain available on `PDO`; where PHP 8.5 moves them to a namespaced
class, the aliases remain present with the same deprecation behavior.

## Remaining acceptance contract

For each accepted change compare DSN grammar, credentials, persistent connection
behavior, diagnostics, versioned symbols, prepare/bind modes, LOBs, rowsets,
metadata, transactions, timeouts and native error shapes with the chosen PHP
reference. Cover normal use and negative security/failure paths. No option may
be accepted inertly.

The existing registry centralizes availability and dispatch; it does not yet
own every attribute, subclass, statement or capability branch. Migrate those
responsibilities incrementally without changing behavior merely to simplify
the registry. Keep optional clients isolated behind bridge profiles and preserve
explicit unavailable-driver diagnostics.

Use [documented divergences](../docs/php/pdo.md#divergences-from-php-src) as the
current gap list. Distinguish unintended differences from deliberate choices,
such as not reproducing an upstream COPY-direction bug. Audit PHP-visible
semantics and actual client-library limits before claiming complete parity.
The original investigation is recoverable from `54c5c3332e`.
