---
title: "lchgrp()"
description: "Changes group ownership of a symlink."
sidebar:
  order: 133
---

## lchgrp()

```php
function lchgrp(string $filename, string $group): bool
```

Changes group ownership of a symlink.

**Parameters**:
- `$filename` (`string`)
- `$group` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/lchgrp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/lchgrp.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `lchgrp` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lchgrp.md).

