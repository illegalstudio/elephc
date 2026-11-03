---
title: "sodium_crypto_box_keypair()"
description: "Generates a random X25519 keypair (secret key followed by public key)."
sidebar:
  order: 1034
---

## sodium_crypto_box_keypair()

```php
function sodium_crypto_box_keypair(): string
```

Generates a random X25519 keypair (secret key followed by public key).

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected sodium prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_keypair.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_keypair.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box_keypair` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sodium_crypto_box_keypair.md).
