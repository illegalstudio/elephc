---
title: "chgrp()"
description: "Changes file group."
sidebar:
  order: 104
---

## chgrp()

```php
function chgrp(string $filename, string $group): bool
```

Changes file group.

**Parameters**:
- `$filename` (`string`)
- `$group` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/chgrp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/chgrp.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `chgrp` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chgrp.md).

