---
title: "hash_hmac()"
description: "Lowers `hash_hmac(algo, data, key, binary?)` through the shared HMAC runtime dispatcher."
sidebar:
  order: 337
---

## hash_hmac()

```php
function hash_hmac(string $algo, string $data, string $key, bool $binary): string
```

Lowers `hash_hmac(algo, data, key, binary?)` through the shared HMAC runtime dispatcher.

**Parameters**:
- `$algo` (`string`)
- `$data` (`string`)
- `$key` (`string`)
- `$binary` (`bool`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_hmac` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_hmac.md).

