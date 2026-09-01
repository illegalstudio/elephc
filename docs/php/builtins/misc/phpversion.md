---
title: "phpversion()"
description: "Returns the targeted PHP language version, or one extension's version."
sidebar:
  order: 334
---

## phpversion()

```php
function phpversion(string $extension = null): string|false
```

Returns the targeted PHP language version, or one extension's version.

**Parameters**:
- `$extension` (`string`), default `null`, optional

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/phpversion.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/phpversion.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `phpversion` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/phpversion.md).
