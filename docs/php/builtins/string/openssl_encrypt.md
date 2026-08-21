---
title: "openssl_encrypt()"
description: "Encrypts data with a supported AES cipher."
sidebar:
  order: 442
---

## openssl_encrypt()

```php
function openssl_encrypt(string $data, string $cipher_algo, string $passphrase, int $options = 0, string $iv = '', mixed $tag = null, string $aad = '', int $tag_length = 16): mixed
```

Encrypts data with a supported AES cipher.

**Parameters**:
- `$data` (`string`)
- `$cipher_algo` (`string`)
- `$passphrase` (`string`)
- `$options` (`int`), default `0`, optional
- `$iv` (`string`), default `''`, optional
- `$tag` (`mixed`), passed by reference, default `null`, optional
- `$aad` (`string`), default `''`, optional
- `$tag_length` (`int`), default `16`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/openssl_encrypt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/openssl_encrypt.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `openssl_encrypt` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/openssl_encrypt.md).
