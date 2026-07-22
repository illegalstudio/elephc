---
title: "hash_hmac()"
description: "Generates a keyed hash value using the HMAC method."
sidebar:
  order: 376
---

## hash_hmac()

```php
function hash_hmac(string $algo, string $data, string $key, bool $binary = false): string
```

Generates a keyed hash value using the HMAC method.

**Parameters**:
- `$algo` (`string`)
- `$data` (`string`)
- `$key` (`string`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_hmac.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_hmac.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_hmac` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_hmac.md).
