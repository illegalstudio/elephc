---
title: "openssl_decrypt()"
description: "Decrypts data with a cipher from the OpenSSL 3.6.4 default-provider catalog."
---

## openssl_decrypt()

```php
function openssl_decrypt(string $data, string $cipher_algo, string $passphrase, int $options = 0, string $iv = '', mixed $tag = null, string $aad = ''): mixed
```

Decrypts data with a cipher from the OpenSSL 3.6.4 default-provider catalog.

**Parameters**:
- `$data` (`string`)
- `$cipher_algo` (`string`)
- `$passphrase` (`string`)
- `$options` (`int`), default `0`, optional
- `$iv` (`string`), default `''`, optional
- `$tag` (`mixed`), default `null`, optional
- `$aad` (`string`), default `''`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `openssl_decrypt` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/openssl_decrypt.md).
