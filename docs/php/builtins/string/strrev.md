---
title: "strrev()"
description: "Reverses a string."
sidebar:
  order: 413
---

## strrev()

```php
function strrev(string $string): string
```

Reverses a string.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strrev.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strrev.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strrev` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strrev.md).

