---
title: "spl_autoload_extensions()"
description: "Register and return default file extensions for spl_autoload."
sidebar:
  order: 330
---

## spl_autoload_extensions()

```php
function spl_autoload_extensions(string $file_extensions = null): string
```

Register and return default file extensions for spl_autoload.

**Parameters**:
- `$file_extensions` (`string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_extensions.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_extensions.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_extensions` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_extensions.md).

