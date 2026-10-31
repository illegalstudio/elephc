---
title: "openssl_get_cipher_methods()"
description: "Returns PHP's OpenSSL 3.6.4 default-provider cipher method names."
sidebar:
  order: 974
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/openssl_get_cipher_methods.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/openssl_get_cipher_methods.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `openssl_get_cipher_methods` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/openssl_get_cipher_methods.md).
