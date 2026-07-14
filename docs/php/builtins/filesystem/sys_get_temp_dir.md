---
title: "sys_get_temp_dir()"
description: "Returns the directory path used for temporary files."
sidebar:
  order: 151
---

## sys_get_temp_dir()

```php
function sys_get_temp_dir(): string
```

Returns the directory path used for temporary files.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/sys_get_temp_dir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/sys_get_temp_dir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sys_get_temp_dir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/sys_get_temp_dir.md).

