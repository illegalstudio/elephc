---
title: "lcfirst()"
description: "Lowercases the first character of a string."
sidebar:
  order: 387
---

## lcfirst()

```php
function lcfirst(string $string): string
```

Lowercases the first character of a string.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/lcfirst.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/lcfirst.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `lcfirst` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/lcfirst.md).
