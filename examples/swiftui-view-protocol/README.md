# PHP-driven SwiftUI — the view-protocol spike

A native app whose entire interface is decided by compiled PHP. Swift draws; it
does not decide. Runs on macOS **and** on iOS, from the same `view.php` and the
same Swift.

```
./run.sh                  # macOS: build and launch
./run.sh --build-only     # macOS: build the .app without launching
./ViewProtocol.app/Contents/MacOS/ViewProtocol --selftest

./run-ios.sh              # iOS: build, install on a booted simulator, launch, screenshot
./run-ios.sh --selftest   # iOS: headless round-trip check inside the simulator
```

macOS needs only the Xcode Command Line Tools — `swiftc` ships with them and
SwiftUI is a system framework. iOS needs full Xcode, an installed runtime and a
booted simulator:

```
xcodebuild -downloadPlatform iOS
xcrun simctl boot "iPhone 17 Pro"
```

Neither path involves an `.xcodeproj`. The library is linked **statically** — the
delivery form an Xcode project would consume — so the exports are ordinary C
symbols rather than `dlsym` lookups, which is also what lets the same Swift run
unchanged on iOS.

## The idea

PHP has no UI toolkit, so "write your app in PHP" is not on the table. What *is*
on the table is the shape React Native and server-driven UI use: **PHP describes
a view tree, a native host renders it.**

```
render_view()  ─────────► {"t":"vstack","children":[ … ]}  ─────────► SwiftUI views
                                                                          │
dispatch("inc") ◄────────────────── button tapped ◄───────────────────────┘
```

`view.php` owns the layout, the labels, the pluralisation and the state.
`ViewProtocolApp.swift` knows four node types and nothing else. Swap `view.php`
and the app changes without recompiling a line of Swift.

This matters for the ahead-of-time story specifically. A template engine has to
*evaluate itself* on the device, which needs a PHP runtime there. A tree
*generator* compiles once and ships as machine code — so this is the one corner
of the UI problem where being AOT costs nothing at all.

## What the spike actually demonstrates

- **A string crosses the boundary in both directions.** `render_view(): string`
  returns a `(ptr, len)` pair the host owns and releases through `elephc_free`;
  `dispatch(string $action)` passes one in.
- **State lives in PHP.** `counter()` uses a function `static`, which persists in
  the loaded library's own memory across host calls. Swift holds no counter.
- **The host stays dumb.** Every string the user sees — including `"2 items"`
  versus `"one item"` — is computed by compiled PHP.

`--selftest` asserts exactly that, headlessly, so the example is verifiable
without a display:

```
initial=nothing yet after++=2 items after-=one item reset=nothing yet
PASS: the view tree, the string ABI and PHP-side state all round-trip
```

## Files

| | |
|---|---|
| `view.php` | the whole application: tree builders, state, action handling |
| `ViewProtocolApp.swift` | JSON → SwiftUI, event dispatch, the self-test |
| `elephc_abi.h` | the C declarations of `ElephcStr` and the exports |
| `run.sh` | macOS: compile both sides, assemble and sign a `.app` |
| `run-ios.sh` | iOS: same, then install, launch and screenshot on a simulator |

## Two things that will bite you

**`ElephcStr` has to be a C type.** Swift rejects a Swift-declared struct in a
`@convention(c)` signature — only a C type carries the guarantee that the value
rides the platform's aggregate-return registers. Hence `elephc_abi.h` and
`-import-objc-header`.

**Returned strings are not NUL-terminated.** They are PHP byte strings and may
contain interior zero bytes, so the returned length is authoritative and
`strlen` is wrong.

**`-sdk` does not reach the link step.** `swiftc` drives `clang` to link, and
that driver defaults to the *host* sysroot — so an iOS build warns *"using
sysroot for 'MacOSX' but targeting 'iPhone'"* unless `-Xclang-linker -isysroot`
passes the SDK through explicitly.

## Scope

Both platforms, one source. The `--selftest` output is identical on macOS and
inside the iOS Simulator, which is the point: nothing in the design is
platform-specific.

macOS on purpose for the original spike: it proved the UI story with the
toolchain that already worked, leaving the iOS SDK as a separate question. That
question is now answered — see `scripts/ios-relink-spike.sh` and
`IOS_TARGET_SPEC.md` — and `run-ios.sh` runs the very same app on a device-class
arm64 simulator.
