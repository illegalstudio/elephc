---
title: "ptr()"
description: "Returns a raw pointer to the given variable."
sidebar:
  order: 289
---

## ptr()

```php
function ptr(mixed $value): mixed
```

Returns a raw pointer to the given variable.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr.md).

