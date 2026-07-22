---
title: "implode()"
description: "Joins array elements into a single string using a separator."
sidebar:
  order: 383
---

## implode()

```php
function implode(string $separator, array $array = null): string
```

Joins array elements into a single string using a separator.

**Parameters**:
- `$separator` (`string`)
- `$array` (`array`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/implode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/implode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `implode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/implode.md).
