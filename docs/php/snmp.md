---
title: "SNMP"
description: "PHP's ext/snmp surface in elephc: SNMP v1/v2c GET, GETNEXT and WALK over the pure-Rust elephc-snmp bridge."
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
| Procedural v1 | `snmp_get()`, `snmp_getnext()`, `snmp_walk()` |
| Procedural v2c | `snmp2_get()`, `snmp2_getnext()`, `snmp2_walk()` |
| Class | `new SNMP(SNMP::VERSION_1 \| SNMP::VERSION_2C, $host, $community, $timeout, $retries)` with `get()`, `getnext()`, `walk()`, `close()` |
| Values | INTEGER, OCTET STRING, NULL, OID, IpAddress, Counter32/64, Gauge32, TimeTicks, Opaque, noSuchObject/noSuchInstance/endOfMibView |
| Hosts | `host` or `host:port`, defaulting to UDP/161 |
| Timeout/retries | ext/snmp's microsecond timeout and retry count (defaults: 1 second, 5 retries) |

`snmp_get()` also accepts an array of OIDs and returns the corresponding list
of values. `snmp_walk()` returns an OID-keyed array with leading-dot numeric
OIDs.

```php
$host = "127.0.0.1:1161";
echo snmp_get($host, "public", ".1.3.6.1.2.1.1.1.0"), "\n";
$tree = snmp2_walk($host, "public", ".1.3.6.1.2.1.1");
```

## Deferred surface

- SNMPv3 and USM, including the PHP 8.6 AES192/AES256 privacy protocols.
- `SNMP::set()` and the SetRequest PDU.
- The 8.6 MIB/output API (`snmp_init_mib()`, `snmp_set_mib_option()`,
  `snmp_set_output_option()`, `snmp_set_string_output_format()`, the
  `Snmp\Mib`/`Snmp\OidOutput`/`Snmp\Output`/`Snmp\StringOutput` classes, and the
  `SNMP::$numeric_index`/`$numeric_timeticks`/`$extended_index`/
  `$dont_print_units`/`$escape_quotes`/`$print_hex_text` flags).
- `snmp_set_quick_print()`, enum/OID output switches, and multi-varbind
  GetBulk requests.

Calling an undeclared name fails at compile time rather than pretending to
work.

## Linking

The prelude carries an `extern "elephc_snmp"` block, so a program that names an
`snmp*` function, `SNMP`, or `SNMP_*` constant links `libelephc_snmp.a`
automatically. `--with-snmp` force-injects the prelude and bridge for indirect
calls.
