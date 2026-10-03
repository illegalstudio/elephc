# Dynamic eval with regex

This example executes a dynamically selected eval fragment that calls
`preg_match()`. Because the regex call is opaque to AOT feature detection, the
project declares managed PCRE2 and compilation explicitly enables the runtime
capability.

From this directory:

```bash
elephc native add pcre2
elephc build --with-regex main.php
./main
```

The expected output is:

```text
1:id:42
```

Without `--with-regex`, the program still compiles, but regex functions are not
exposed to dynamic eval and a call to `preg_match()` fails at runtime.
