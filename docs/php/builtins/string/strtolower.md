---
title: "strtolower()"
description: "Converts a string to lowercase."
sidebar:
  order: 416
---

## strtolower()

```php
function strtolower(string $string): string
```

Converts a string to lowercase.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strtolower.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strtolower.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strtolower` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strtolower.md).

