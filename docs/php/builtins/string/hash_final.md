---
title: "hash_final()"
description: "Finalizes an incremental hash and returns the digest string."
sidebar:
  order: 373
---

## hash_final()

```php
function hash_final(resource $context, bool $binary = false): string
```

Finalizes an incremental hash and returns the digest string.

**Parameters**:
- `$context` (`resource`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_final.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_final.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_final` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_final.md).

