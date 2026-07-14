---
title: "ptr_sizeof()"
description: "Returns the byte size of the named pointer target type."
sidebar:
  order: 299
---

## ptr_sizeof()

```php
function ptr_sizeof(string $type): int
```

Returns the byte size of the named pointer target type.

**Parameters**:
- `$type` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_sizeof.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_sizeof.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_sizeof` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_sizeof.md).

