# Security Policy

## Supported versions

Security fixes land on `main` and ship in the next release. Only the latest release is supported; please reproduce
on it, or on `main`, before reporting.

## Reporting a vulnerability

Please do not open a public issue, discussion or pull request for a vulnerability. Report it privately, in either
of these ways:

* GitHub private vulnerability reporting: [open a draft advisory](https://github.com/illegalstudio/elephc/security/advisories/new).
* Email: [sec@elephc.dev](mailto:sec@elephc.dev).

Please include:

* The elephc version or commit, the target (for example `linux-x86_64` or `macos-aarch64`) and the compiler flags.
* A minimal PHP program and the input that triggers the problem.
* What happens (crash, corrupted output, sanitizer or `--heap-debug` report) and what you expected.

## Scope

The PHP source a developer compiles is trusted; the data a compiled program processes at run time is not. Memory
corruption in the generated runtime or in a bridge crate, a miscompilation that turns valid PHP into memory-unsafe
code, and integrity problems in `elephc native` (checksum or path checks) are in scope. The raw-memory extensions
(`ptr`, `extern`, `buffer<T>`, `packed class`, FFI) are unsafe by design. [`.oss-scanner/threat_model.md`](.oss-scanner/threat_model.md)
describes the attack surface and how we rate severity in more detail.
