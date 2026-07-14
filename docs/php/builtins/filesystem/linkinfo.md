---
title: "linkinfo()"
description: "Gets information about a link."
sidebar:
  order: 136
---

## linkinfo()

```php
function linkinfo(string $path): int
```

Gets information about a link.

**Parameters**:
- `$path` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/linkinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/linkinfo.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `linkinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/linkinfo.md).

