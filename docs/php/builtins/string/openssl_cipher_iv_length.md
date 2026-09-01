---
title: "openssl_cipher_iv_length()"
description: "Returns the IV length for a supported cipher."
sidebar:
  order: 445
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/openssl_cipher_iv_length.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/openssl_cipher_iv_length.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `openssl_cipher_iv_length` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/openssl_cipher_iv_length.md).
