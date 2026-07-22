---
title: "dirname()"
description: "Returns a parent directory's path."
sidebar:
  order: 111
---

## dirname()

```php
function dirname(string $path, int $levels = 1): string
```

Returns a parent directory's path.

**Parameters**:
- `$path` (`string`)
- `$levels` (`int`), default `1`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/dirname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/dirname.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `dirname` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/dirname.md).
