---
title: "openssl_get_cipher_methods()"
description: "Returns PHP's OpenSSL 3.6.4 default-provider cipher method names."
---

## openssl_get_cipher_methods()

```php
function openssl_get_cipher_methods(bool $aliases = false): array
```

Returns PHP's OpenSSL 3.6.4 default-provider cipher method names.

**Parameters**:
- `$aliases` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `openssl_get_cipher_methods` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/openssl_get_cipher_methods.md).
