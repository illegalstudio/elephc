---
title: "hash()"
description: "Generates a hash value using the given algorithm."
---

## hash()

```php
function hash(string $algo, string $data, bool $binary = false): string
```

Generates a hash value using the given algorithm.

**Parameters**:
- `$algo` (`string`)
- `$data` (`string`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash.md).
