---
title: "random_bytes()"
description: "Get a cryptographically secure random string of the given length."
sidebar:
  order: 283
---

## random_bytes()

```php
function random_bytes(int $length): string
```

Get a cryptographically secure random string of the given length.

**Parameters**:
- `$length` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/random_bytes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/random_bytes.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `random_bytes` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/random_bytes.md).
