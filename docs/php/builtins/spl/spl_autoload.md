---
title: "spl_autoload()"
description: "Default implementation for __autoload()."
sidebar:
  order: 343
---

## spl_autoload()

```php
function spl_autoload(string $class, string $file_extensions = null): void
```

Default implementation for __autoload().

**Parameters**:
- `$class` (`string`)
- `$file_extensions` (`string`), default `null`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload.md).
