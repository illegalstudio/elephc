---
title: "php_uname()"
description: "Returns information about the operating system PHP is running on."
sidebar:
  order: 295
---

## php_uname()

```php
function php_uname(string $mode = 'a'): string
```

Returns information about the operating system PHP is running on.

**Parameters**:
- `$mode` (`string`), default `'a'`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/php_uname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/php_uname.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `php_uname` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/php_uname.md).

