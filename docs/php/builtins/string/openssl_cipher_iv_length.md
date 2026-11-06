---
title: "openssl_cipher_iv_length()"
description: "Returns the IV length for a supported cipher."
---

## openssl_cipher_iv_length()

```php
function openssl_cipher_iv_length(string $cipher_algo): mixed
```

Returns the IV length for a supported cipher.

**Parameters**:
- `$cipher_algo` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `openssl_cipher_iv_length` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/openssl_cipher_iv_length.md).
