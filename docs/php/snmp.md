---
title: "SNMP"
description: "PHP's ext/snmp surface in elephc: SNMP v1/v2c/v3 GET, GETNEXT, SET and GETBULK walks over the pure-Rust elephc-snmp bridge."
sidebar:
  order: 25
---

elephc implements the first `ext/snmp` client slice through an injected PHP
prelude backed by the pure-Rust `elephc-snmp` bridge. The bridge speaks SNMP
v1 and v2c over UDP with hand-written BER encoding/decoding, so it needs no
system net-snmp library.

## Delivered surface

| Area | Functions |
|---|---|
| Procedural v1 | `snmp_get()`, `snmp_getnext()`, `snmp_walk()`, `snmpset()` |
| Procedural v2c | `snmp2_get()`, `snmp2_getnext()`, `snmp2_walk()`, `snmp2_set()` |
| Class | `new SNMP(SNMP::VERSION_1 \| SNMP::VERSION_2C \| SNMP::VERSION_3, $host, $community, $timeout, $retries)` with `get()`, `getnext()`, `walk()`, `set()`, `getErrno()`, `getError()`, `$info`, `close()` |
| Walks | GETNEXT for v1 and GETBULK for v2c/v3, with `maxRepetitions` (default 20) and `nonRepeaters` |
| SET types | `i` INTEGER, `u` Gauge32, `t` TimeTicks, `s`/`d` OCTET STRING, `x`/`b` hex OCTET STRING, `n` NULL, `o` OBJECT IDENTIFIER, `a` IpAddress |
| SNMPv3 USM | `snmp3_get()`, `snmp3_getnext()`, `snmp3_walk()`, `snmp3_set()`, `SNMP::setSecurity()`, MD5/SHA-1/SHA-224/SHA-256/SHA-384/SHA-512 auth, DES/AES-128/AES-192/AES-256 privacy (Blumenthal and Cisco/Reeder key extension) |
| MIBs | `snmp_read_mib()`, `snmp_init_mib()`, symbolic OID resolution (`MODULE::name.instance`, `name.instance`), `Snmp\OidOutput::{Suffix,Module,Full,Ucd,Numeric}` rendering, `INTEGER`/`BITS` enum labels (`snmp_set_enum_print()`/`SNMP::$enum_print`), `UNITS` suffixes, and table `INDEX` rendering with numeric/extended index options |
| PHP 8.6 output API | `Snmp\Mib`, `Snmp\OidOutput`, `Snmp\Output`, `Snmp\StringOutput`; `snmp_init_mib()`, `snmp_set_mib_option()`, `snmp_set_output_option()`, `snmp_set_string_output_format()`, `snmp_set_oid_output_format()` and the `SNMP` instance flags |
| Values | INTEGER, OCTET STRING, NULL, OID, IpAddress, Counter32/64, Gauge32, TimeTicks, Opaque, noSuchObject/noSuchInstance/endOfMibView |
| Value retrieval | `SNMP_VALUE_LIBRARY` (default, net-snmp `STRING:`/`INTEGER:`/`Timeticks:`/`Counter32:` labels), `SNMP_VALUE_PLAIN` (raw values), and `SNMP_VALUE_OBJECT` (`{type, value}` objects); `snmp_set_quick_print()`, `snmp_set_enum_print()`, `Snmp\Output::NumericTimeticks`, `Snmp\Output::EscapeQuotes`, `Snmp\Output::HexText` and `snmp_set_string_output_format()` (`Guess`/`Ascii`/`Hex`) alter the formatted output. Plain OCTET STRINGs are quoted; `Hex-STRING` renders uppercase bytes and can append the ASCII form. MIB `DISPLAY-HINT` textual conventions (such as `DisplayString`) print without quotes. OID values and walk keys use the configured/default module-qualified MIB names. |
| Hosts | `host` or `host:port`, defaulting to UDP/161 |
| Timeout/retries | ext/snmp's microsecond timeout and retry count (defaults: 1 second, 5 retries) |

`snmp_get()` also accepts an array of OIDs and returns the corresponding list
of values. `snmp_walk()` returns an OID-keyed array; v2c/v3 walks use GETBULK
and v1 falls back to GETNEXT. `SNMP::$info` reports the session hostname,
timeout and retry count.

After `snmp_read_mib()`, symbolic OIDs and name rendering work in the same
process:

```php
snmp_read_mib("MY-MIB.txt");
snmp_set_oid_output_format(\Snmp\OidOutput::Module);
$tree = snmp2_walk($host, "public", "myObject");
```

```php
$host = "127.0.0.1:1161";
echo snmp_get($host, "public", ".1.3.6.1.2.1.1.1.0"), "\n";
$tree = snmp2_walk($host, "public", ".1.3.6.1.2.1.1");
var_dump(snmp2_set($host, "public", ".1.3.6.1.2.1.1.5.0", "s", "new name"));
```

## MIB rendering

`snmp_read_mib()` captures `UNITS` clauses and table `INDEX` metadata.
Table instances render their indexes like net-snmp: integer indexes as `.1`,
string indexes as `."name"`, IPv4 indexes as `.127.0.0.1`, and OID indexes with
their raw length-prefixed arcs. `snmp_set_output_option()` toggles
`Snmp\Output::NumericIndex` (raw sub-identifiers) and
`Snmp\Output::ExtendedIndex` (`[1]`, `[STRING: name]`, `[1.2.3.4]`, or
`[MODULE::name]`). `Snmp\Output::DontPrintUnits` suppresses the `UNITS` suffix
on INTEGER, Counter, Gauge, Counter64 and TimeTicks values in library, object
and quick modes; plain mode never includes units.

`snmp_set_mib_option()` controls net-snmp's parser toggles:
`Snmp\Mib::AllowUnderscores` accepts underscores in identifiers,
`Snmp\Mib::CommentTerm` ends `--` comments at the next `--` instead of the end
of the line, and `Snmp\Mib::Replace` replaces already-registered objects when a
MIB is read again.

`TEXTUAL-CONVENTION` definitions in a loaded MIB are parsed, so their base
syntax and `DISPLAY-HINT` drive index rendering and string formatting.
Standard conventions (`DisplayString`, `SnmpAdminString`, `MacAddress`,
`PhysAddress`, `InterfaceIndex`, `InetAddress`, …) are seeded. A convention
imported from a MIB that was never loaded falls back to the raw numeric
instance suffix; load that module with `snmp_read_mib()` to resolve it.

Calling an undeclared name fails at compile time rather than pretending to
work.

## Linking

The prelude carries an `extern "elephc_snmp"` block, so a program that names an
`snmp*` function, `SNMP`, or `SNMP_*` constant links `libelephc_snmp.a`
automatically. `--with-snmp` force-injects the prelude and bridge for indirect
calls.
