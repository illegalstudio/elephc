---
title: "sodium_crypto_box_publickey()"
description: "Extracts the public key from a crypto_box keypair."
sidebar:
  order: 1033
---

## sodium_crypto_box_publickey()

```php
function sodium_crypto_box_publickey(string $key_pair): string
```

Extracts the public key from a crypto_box keypair.

**Parameters**:
- `$key_pair` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected sodium prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_publickey.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_publickey.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box_publickey` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sodium_crypto_box_publickey.md).
