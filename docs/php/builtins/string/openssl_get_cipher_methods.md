---
title: "openssl_get_cipher_methods()"
description: "Returns the supported OpenSSL cipher method names."
sidebar:
  order: 448
---

## openssl_get_cipher_methods()

```php
function openssl_get_cipher_methods(bool $aliases = false): array
```

Returns the supported OpenSSL cipher method names.

**Parameters**:
- `$aliases` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/openssl_get_cipher_methods.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/openssl_get_cipher_methods.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `openssl_get_cipher_methods` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/openssl_get_cipher_methods.md).
