---
title: "hash_algos()"
description: "Returns an array of supported hashing algorithm names."
sidebar:
  order: 370
---

## hash_algos()

```php
function hash_algos(): array
```

Returns an array of supported hashing algorithm names.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_algos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_algos.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_algos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_algos.md).

