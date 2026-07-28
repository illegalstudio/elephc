---
title: "ucfirst()"
description: "Uppercases the first character of a string."
sidebar:
  order: 430
---

## ucfirst()

```php
function ucfirst(string $string): string
```

Uppercases the first character of a string.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ucfirst.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ucfirst.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ucfirst` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ucfirst.md).
