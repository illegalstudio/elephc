---
title: "sodium_crypto_box_seal()"
description: "Encrypts a message anonymously to a public key (sealed box)."
sidebar:
  order: 1034
---

## sodium_crypto_box_seal()

```php
function sodium_crypto_box_seal(string $message, string $public_key): string
```

Encrypts a message anonymously to a public key (sealed box).

**Parameters**:
- `$message` (`string`)
- `$public_key` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected sodium prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_seal.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_seal.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_crypto_box_seal` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sodium_crypto_box_seal.md).
