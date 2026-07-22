---
title: "hash_init()"
description: "Initialize an incremental hashing context."
sidebar:
  order: 377
---

## hash_init()

```php
function hash_init(string $algo, int $flags = 0, string $key = ''): mixed
```

Initialize an incremental hashing context.

**Parameters**:
- `$algo` (`string`)
- `$flags` (`int`), default `0`, optional
- `$key` (`string`), default `''`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_init.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_init.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_init.md).
