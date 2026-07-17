# PDO full maintained-PHP parity

- [x] Freeze the PHP 8.2–8.5 PDO reference matrix from versioned php-src/PECL sources.
- [x] Replace availability and DSN dispatch conditionals with a single compiled-driver registry.
- [ ] Migrate driver-specific attributes, subclasses, statements, and capability hooks to the registry.
- [x] Implement `pdo.dsn.*` alias resolution with PHP-compatible configuration precedence.
- [ ] Remove the documented common PDO/PDOStatement divergences.
- [x] Add PDO_DBLIB / `Pdo\Dblib` with versioned constants and FreeTDS live tests.
- [x] Add PDO_FIREBIRD / `Pdo\Firebird` with versioned constants and Firebird live tests.
- [x] Add PDO_ODBC / `Pdo\Odbc` with unixODBC/iODBC live tests.
- [x] Add PDO_OCI compatibility for PHP 8.2–8.5, including the post-8.3 PECL split.
- [x] Add the maintained external PDO_CUBRID, PDO_IBM, PDO_INFORMIX, and PDO_SQLSRV surfaces.
- [ ] Validate every available backend on macOS AArch64, Linux AArch64, and Linux x86_64.
- [ ] Regenerate the complete documentation/compatibility report and close every recorded gap.

Current qualification boundary: all eleven drivers in the frozen matrix are implemented,
and every optional profile builds in the three-target CI matrix. PostgreSQL, MySQL,
DBLIB, Firebird, ODBC, SQLSRV, OCI, and CUBRID have Linux live acceptance; Informix and
IBM retain unit and compiled-surface coverage until redistributable proprietary Client SDK
and server fixtures are available. Cross-target live execution and the documented common
PDO divergences therefore remain open and prevent a literal 100% certification claim.

## Normative scope

The normative language versions are the PHP branches currently supported on 2026-07-16:
PHP 8.2, 8.3, 8.4, and 8.5. PHP 8.6 remains an elephc preview target and inherits the
latest known surface until its php-src branch becomes stable. PHP 8.0/8.1 stay supported
as historical elephc targets but do not drive new compatibility decisions.

The in-tree php-src drivers are `pdo_dblib`, `pdo_firebird`, `pdo_mysql`, `pdo_odbc`,
`pdo_pgsql`, and `pdo_sqlite`; `pdo_oci` is in php-src through PHP 8.3 and is maintained
externally afterwards. The PHP manual also lists PDO_CUBRID, PDO_IBM, PDO_INFORMIX, and
PDO_SQLSRV. Those external drivers are in scope: their upstream extension sources and
released binaries, not generic PDO behavior alone, define their driver-specific contract.

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

## Compatibility contract

For every driver and PHP target, parity covers:

- DSN grammar, credential precedence, connection/persistence behavior, and error shape;
- constants, `Pdo\*` subclasses, method signatures, attributes, and availability by version;
- placeholder parsing, native/emulated prepares, binds, LOBs, rowsets, metadata, and types;
- transaction/autocommit behavior, quoting, timeout semantics, and driver-native errors;
- build-time and runtime client-library version boundaries;
- `PDO::getAvailableDrivers()` / `pdo_drivers()` reflecting the actually linked drivers;
- positive live-server tests and negative security/failure tests.

No option may be accepted inertly. A client feature that cannot be honored must fail with
the same PHP-visible diagnostic as its reference driver. Optional proprietary clients must
be isolated behind bridge features, retain explicit diagnostics when unavailable, and must
not weaken the first-class supported-target policy for the default build.

## Architecture

The current monolithic `Conn`/`Stmt` enums and PHP string comparisons are replaced
incrementally by a single registry describing each driver name, DSN prefixes/aliases,
version availability, library feature, attributes, subclasses, and capability hooks. The
existing SQLite/PostgreSQL/MySQL implementations migrate first without semantic changes;
new drivers then plug into that boundary rather than expanding scattered match trees.

System-client drivers remain optional bridge profiles. Prefer protocol-native Rust clients
when they reproduce the PHP client's semantics on every target; otherwise call the same C
client as PHP (as the libpq GSS profile does). CI must build both the standalone default and
each system-client profile.

## `pdo.dsn.*` aliases

Aliases are runtime configuration in PHP, so compile-time substitution is insufficient.
The bridge will resolve a colonless PDO DSN from PHP-style configuration sources before
driver dispatch. The implementation must preserve PHP's distinction between an undefined
alias (`invalid data source name`) and a resolved alias whose driver is unavailable
(`could not find driver`), credential precedence, persistent-pool keys, and `uri:` handling.

Configuration discovery and precedence will be shared with the compiler's future INI
surface, but the PDO implementation must at minimum honor `PHPRC`, the loaded `php.ini`,
scan-directory fragments, and `pdo.dsn.<name>` last-assignment semantics. Tests use isolated
temporary configurations and never depend on a developer machine's PHP installation.

## Delivery order

1. Registry + aliases + common divergences, because every subsequent driver depends on it.
2. DBLIB and Firebird, both bundled and independently runnable in Linux CI.
3. ODBC as the shared system-client substrate.
4. OCI and the externally maintained drivers, with hermetic CI where redistribution permits.
5. Cross-version/source audit and complete supported-target verification.

Each phase lands green independently with examples and live CI. “100%” is claimed only when
the generated audit contains no unexplained missing symbol, option, version gate, diagnostic,
or unexecuted live path.
