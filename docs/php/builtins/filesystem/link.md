---
title: "link()"
description: "Creates a hard link."
sidebar:
  order: 137
---

## link()

```php
function link(string $target, string $link): bool
```

Creates a hard link.

**Parameters**:
- `$target` (`string`)
- `$link` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/link.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/link.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `link` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/link.md).
