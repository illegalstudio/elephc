# iOS sandbox probe

Measures which kernel-facing operations actually work inside a real iOS app
sandbox, by running compiled PHP there and reporting what succeeded.

```
./run.sh simulator     # booted simulator, no signing needed
./run.sh device        # physical device, needs a signing identity
```

## Why this exists

**The iOS Simulator runs on the macOS kernel.** A simulator app is a native
macOS process that loads the iOS frameworks; there is no iOS kernel involved. So
when elephc's `svc #0x80` executes there, it hits the *macOS* syscall table —
precisely the one elephc was written against.

That means the simulator validates codegen, the C ABI, string marshaling,
ownership, Mach-O and static linking — all of which it did — but says nothing
about the device sandbox.

elephc emits **225 raw syscalls**. Roughly 161 of them
(`write`/`read`/`close`/`exit`/`lseek`/`fstat`/`gettimeofday`) are
unconditionally safe. The remaining ~26 are path-based and network, and those
are the open question this probe turns into facts.

The simulator report makes the point by itself:

```
OK   outside./tmp write         permitted
OK   outside./etc/hosts stat    readable
OK   env.getcwd                 /Users/…/CoreSimulator/Devices/…/data
OK   env.getenv HOME            /Users/…/CoreSimulator/Devices/…/data
```

None of that should hold on a device. **Run both modes and diff the reports** —
the difference is the answer.

## What it checks

| group | what it exercises |
|---|---|
| `container.*` | `open`/`write`/`lseek`/`read`/`close`/`stat`/`unlink` inside the app's own temporary directory |
| `outside.*` | paths beyond the container — expected to be denied on a device |
| `env.*` | `getcwd`, `sys_get_temp_dir`, `getenv` — all of which differ sharply on device |
| `time.*` | the clock |
| `net.*` | DNS resolution, subject to App Transport Security |

Nothing aborts on failure: a probe that dies on the first denial measures one
thing instead of all of them.

## Running on a device

The script builds everything and stops at the signature, because a device
build needs a certificate and a provisioning profile that only your Apple ID can
issue. The reliable way to create both is to let Xcode provision the device once:

1. Xcode ▸ Settings ▸ Accounts, add your Apple ID — a free one works.
2. Connect and trust the iPhone.
3. Create any empty iOS App project, set its team, Run it on the device. That
   issues an `Apple Development` certificate and a provisioning profile.
4. Re-run `./run.sh device`.

If the profile is not a wildcard, pass a matching identifier:

```
ELEPHC_PROBE_BUNDLE_ID=com.example.probe ./run.sh device
```

The script then embeds the profile, extracts its entitlements — signing with
anything the profile does not grant is rejected at *install* time, not at sign
time — installs through `devicectl` and launches with `--console`, so the report
comes back on stdout as well as appearing on screen.

## What a consumer learns from this

**An elephc static library is not self-contained.** Any PHP touching the
filesystem reaches `__rt_fopen_maybe_phar`, which pulls in the `elephc-phar`
bridge, which pulls in bzip2. `Emit::Staticlib` deliberately leaves bridges and
native packages to the consuming project, exactly as a C library leaves its
dependencies to its consumer — so an Xcode target linking elephc also links:

- the bridge staticlibs it uses (`libelephc_phar.a` here), **cross-compiled for
  the same target** — `rustup target add aarch64-apple-ios`, then
  `cargo build -p elephc-phar --target aarch64-apple-ios`;
- the system libraries those need (`-lbz2 -lz` here).

Which bridges you need depends on which PHP surface you use. A program that only
does arithmetic and string work needs none — that is why
`examples/swiftui-view-protocol` links without any.

## Files

| | |
|---|---|
| `probe.php` | every check, and the report format |
| `ProbeApp.swift` | renders the report and prints it to stdout |
| `probe_abi.h` | C declarations of `ElephcStr` and the entry points |
| `run.sh` | build, bundle, sign, install, launch — both modes |
