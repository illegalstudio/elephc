# Hello, preg

This is the smallest managed-native regex project. The source uses `preg_match()`,
so a final link needs the exact PCRE2 artifact declared by `elephc.toml` and pinned
for every supported target in `elephc.lock`.

It also captures word-boundary byte offsets with `preg_match_all()`, including
the empty match at the end of the subject. Expected output:

```text
Hello, preg!
Word boundaries: 0 5 6 10
```

From this directory:

```bash
elephc native add pcre2
elephc main.php
./main
```

`native add` is idempotent with the committed manifest and lock: it verifies or
materializes the host artifact without falling back to a system PCRE2. In CI, use
`elephc native install --locked --target <target>` instead.
