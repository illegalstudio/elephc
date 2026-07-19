---
title: "pathinfo()"
description: "Returns information about a file path."
sidebar:
  order: 139
---

## pathinfo()

```php
function pathinfo(string $path, int $flags = 15): array
```

Returns information about a file path.

**Parameters**:
- `$path` (`string`)
- `$flags` (`int`), default `15`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/pathinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/pathinfo.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `pathinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/pathinfo.md).

