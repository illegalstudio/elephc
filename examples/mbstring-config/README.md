# Configured label counting

`main.php` counts the label Café using the configured internal encoding.
The default UTF-8 setting counts four characters; 8bit counts five bytes.
It also reads the configured language with `ini_get`, changes it for the current
request with `ini_set`, and restores its startup value with `ini_restore`.
Run these commands from this example directory:

```sh
elephc native add pcre2
elephc --ini default_charset=8bit main.php
./main
```

`library.php` exposes the same encoding and counting behavior to a C host:

```sh
elephc --emit cdylib --ini default_charset=8bit library.php
```

Include the generated `liblibrary.h` and link `liblibrary.so` on Linux or
`liblibrary.dylib` on macOS. The host may call
`elephc_init()` explicitly; otherwise the first export initializes the settings.
Release the buffer returned by `label_encoding()` with `elephc_free()`.
`label_length()` returns five for the UTF-8 bytes of Café in this configuration.

With `--emit staticlib`, the output is `liblibrary.a`. The host must also link
the matching `libelephc_mbstring.a` bridge and the managed PCRE2 archives in this
order: `libelephc_pcre2_shim.a`, `libpcre2-posix.a`, `libpcre2-8.a`. On Linux,
append `-lm -ldl -lpthread`. These dependencies are separate from the generated
static archive; the shared-library build links them automatically.
