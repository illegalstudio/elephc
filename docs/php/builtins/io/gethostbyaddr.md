---
title: "gethostbyaddr()"
description: "Gets the Internet host name corresponding to a given IP address."
sidebar:
  order: 183
---

## gethostbyaddr()

```php
function gethostbyaddr(string $ip): mixed
```

Gets the Internet host name corresponding to a given IP address.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/gethostbyaddr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/gethostbyaddr.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gethostbyaddr` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gethostbyaddr.md).
