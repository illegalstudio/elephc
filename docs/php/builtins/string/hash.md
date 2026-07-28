---
title: "hash()"
description: "Generates a hash value using the given algorithm."
sidebar:
  order: 378
---

## hash()

```php
function hash(string $algo, string $data, bool $binary = false): string
```

Generates a hash value using the given algorithm.

**Parameters**:
- `$algo` (`string`)
- `$data` (`string`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash.md).
