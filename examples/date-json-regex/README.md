# Date, JSON, and managed regex example

This example combines date/time formatting, JSON encode/decode, and PCRE2-backed
regex functions. PCRE2 is a curated native dependency declared in
`elephc.toml` and pinned by `elephc.lock`.

From the repository root, either use the committed lock directly:

```bash
cd examples/date-json-regex
elephc native install --locked
elephc main.php
./main
```

Or exercise the initial project workflow (re-adding the same exact dependency
is idempotent):

```bash
cd examples/date-json-regex
elephc native add pcre2
elephc main.php
./main
```

After the verified source/artifact is cached, installation can be checked
without network access:

```bash
elephc native install --locked --offline
```

The manifest and lock are project inputs and should stay committed. The
generated assembly, object, and `main` binary are ignored; the global native
cache is outside this directory.
