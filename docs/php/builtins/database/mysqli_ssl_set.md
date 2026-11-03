---
title: "mysqli_ssl_set()"
description: "Records the TLS key, certificate and CA files a MYSQLI_CLIENT_SSL connection uses."
sidebar:
  order: 173
---

## mysqli_ssl_set()

```php
function mysqli_ssl_set(mixed $mysql, ?string $key, ?string $certificate, ?string $ca_certificate, ?string $ca_path, ?string $cipher_algos): bool
```

Records the TLS key, certificate and CA files a MYSQLI_CLIENT_SSL connection uses.

**Parameters**:
- `$mysql` (`mixed`)
- `$key` (`?string`)
- `$certificate` (`?string`)
- `$ca_certificate` (`?string`)
- `$ca_path` (`?string`)
- `$cipher_algos` (`?string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_ssl_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_ssl_set.md).
