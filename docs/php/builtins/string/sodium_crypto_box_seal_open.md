---
title: "sodium_crypto_box_seal_open()"
description: "Opens a sealed box with a keypair, returning false when it does not authenticate."
sidebar:
  order: 1035
---

## sodium_crypto_box_seal_open()

```php
function sodium_crypto_box_seal_open(string $ciphertext, string $key_pair): mixed
```

Opens a sealed box with a keypair, returning false when it does not authenticate.

**Parameters**:
- `$ciphertext` (`string`)
- `$key_pair` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected sodium prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_seal_open.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_seal_open.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box_seal_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sodium_crypto_box_seal_open.md).
