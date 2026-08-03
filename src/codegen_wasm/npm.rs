//! Purpose:
//! Writes the Node.js ESM package produced by `--emit npm` for a compiled
//! wasm32-wasi command module.
//!
//! Called from:
//! - `crate::pipeline::emit_wasm_artifacts()` after WAT has been encoded to WASM.
//!
//! Key details:
//! - The generated loader uses Node's built-in `node:wasi` preview1 runtime.
//! - The package keeps the WASM binary beside the loader and exposes a reusable
//!   asynchronous `run()` API as well as a directly executable `index.mjs`.

use serde_json::json;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const WASM_FILENAME: &str = "module.wasm";
const MAX_NPM_PACKAGE_NAME_BYTES: usize = 214;
static STAGING_SEQ: AtomicU64 = AtomicU64::new(0);

/// Writes a complete Node.js ESM package containing `wasm_bytes`.
pub fn write_package(
    package_dir: &Path,
    source_stem: &str,
    wasm_bytes: &[u8],
) -> io::Result<()> {
    fs::create_dir_all(package_dir)?;
    let package_name = npm_package_name(source_stem);
    let package_json = serde_json::to_string_pretty(&json!({
        "name": &package_name,
        "version": "0.0.0",
        "description": format!("wasm32-wasi command compiled from {source_stem}.php by elephc"),
        "type": "module",
        "exports": {
            ".": {
                "types": "./index.d.ts",
                "import": "./index.mjs",
                "default": "./index.mjs"
            }
        },
        "types": "./index.d.ts",
        "files": [
            "index.mjs",
            "index.d.ts",
            WASM_FILENAME,
            "README.md"
        ],
        "engines": {
            "node": ">=20.0.0"
        }
    }))
    .map_err(io::Error::other)?;

    fs::write(package_dir.join(WASM_FILENAME), wasm_bytes)?;
    fs::write(package_dir.join("package.json"), format!("{package_json}\n"))?;
    fs::write(package_dir.join("index.mjs"), loader_source(&package_name))?;
    fs::write(package_dir.join("index.d.ts"), type_declarations())?;
    fs::write(
        package_dir.join("README.md"),
        readme_source(&package_name, source_stem),
    )?;
    Ok(())
}

/// A complete package prepared beside its final destination but not yet visible.
///
/// The artifact publisher keeps the previous package in `backup` until every
/// sibling artifact has committed, allowing a later failure to roll the whole
/// publication back.
pub(super) struct StagedPackage {
    destination: PathBuf,
    staging: PathBuf,
    backup: PathBuf,
    had_previous: bool,
    previous_backed_up: bool,
    published: bool,
    finalized: bool,
}

impl StagedPackage {
    /// Moves the staged package into place while retaining any previous package
    /// in its private backup path for a possible transaction rollback.
    pub(super) fn publish(&mut self) -> io::Result<()> {
        if self.had_previous {
            fs::rename(&self.destination, &self.backup)?;
            self.previous_backed_up = true;
        }

        if let Err(publish_error) = fs::rename(&self.staging, &self.destination) {
            if self.previous_backed_up {
                if let Err(restore_error) = fs::rename(&self.backup, &self.destination) {
                    return Err(io::Error::other(format!(
                        "publishing package failed ({publish_error}); restoring the previous package failed ({restore_error})"
                    )));
                }
                self.previous_backed_up = false;
            }
            return Err(publish_error);
        }
        self.published = true;
        Ok(())
    }

    /// Restores the pre-publication state and removes every staging or backup
    /// path owned by this package transaction.
    pub(super) fn rollback(&mut self) -> io::Result<()> {
        let mut errors = Vec::new();

        if self.published {
            if let Err(error) = remove_path_if_exists(&self.destination) {
                errors.push(format!(
                    "removing newly published package '{}': {error}",
                    self.destination.display()
                ));
            } else {
                self.published = false;
            }
        }

        if self.previous_backed_up && !self.destination.exists() {
            if let Err(error) = fs::rename(&self.backup, &self.destination) {
                errors.push(format!(
                    "restoring previous package '{}': {error}",
                    self.destination.display()
                ));
            } else {
                self.previous_backed_up = false;
            }
        }

        if let Err(error) = remove_path_if_exists(&self.staging) {
            errors.push(format!(
                "removing package staging path '{}': {error}",
                self.staging.display()
            ));
        }
        if !self.previous_backed_up {
            if let Err(error) = remove_path_if_exists(&self.backup) {
                errors.push(format!(
                    "removing package backup path '{}': {error}",
                    self.backup.display()
                ));
            }
        }

        if errors.is_empty() {
            self.finalized = true;
            Ok(())
        } else {
            Err(io::Error::other(errors.join("; ")))
        }
    }

    /// Marks the published package as committed so cleanup failures cannot
    /// trigger an independent rollback after sibling artifacts have committed.
    pub(super) fn commit(&mut self) {
        self.previous_backed_up = false;
        self.finalized = true;
    }

    /// Removes retained staging and backup paths after the enclosing
    /// multi-artifact transaction has committed every destination.
    pub(super) fn cleanup(&mut self) -> io::Result<()> {
        remove_path_if_exists(&self.staging)?;
        remove_path_if_exists(&self.backup)?;
        Ok(())
    }
}

impl Drop for StagedPackage {
    /// Best-effort safety net for early returns; explicit callers still surface
    /// rollback failures through `rollback`.
    fn drop(&mut self) {
        if !self.finalized {
            let _ = self.rollback();
        }
    }
}

/// Builds a complete package in a unique sibling staging directory.
///
/// Existing destinations must be real directories. Files and symlinks are
/// rejected before staging so compiling cannot rename unrelated user data.
pub(super) fn stage_package(
    package_dir: &Path,
    source_stem: &str,
    wasm_bytes: &[u8],
) -> io::Result<StagedPackage> {
    let had_previous = match fs::symlink_metadata(package_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => true,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "NPM package destination '{}' exists and is not a directory",
                    package_dir.display()
                ),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };

    let staging = unique_sibling_path(package_dir, "stage");
    let backup = unique_sibling_path(package_dir, "backup");
    if let Err(write_error) = write_package(&staging, source_stem, wasm_bytes) {
        return match remove_path_if_exists(&staging) {
            Ok(()) => Err(write_error),
            Err(cleanup_error) => Err(io::Error::other(format!(
                "building package failed ({write_error}); cleaning staging failed ({cleanup_error})"
            ))),
        };
    }

    Ok(StagedPackage {
        destination: package_dir.to_path_buf(),
        staging,
        backup,
        had_previous,
        previous_backed_up: false,
        published: false,
        finalized: false,
    })
}

/// Publishes the NPM package through sibling staging and backup directories so
/// a failed write restores the previous package and leaves no transaction debris.
#[cfg(test)]
pub fn write_package_atomic(
    package_dir: &Path,
    source_stem: &str,
    wasm_bytes: &[u8],
) -> io::Result<()> {
    let mut package = stage_package(package_dir, source_stem, wasm_bytes)?;
    if let Err(publish_error) = package.publish() {
        return match package.rollback() {
            Ok(()) => Err(publish_error),
            Err(rollback_error) => Err(io::Error::other(format!(
                "publishing package failed ({publish_error}); rollback failed ({rollback_error})"
            ))),
        };
    }
    package.commit();
    package.cleanup()
}

/// Returns a process- and sequence-unique sibling path that does not currently
/// exist, avoiding deletion of stale or unrelated paths on name collision.
fn unique_sibling_path(destination: &Path, role: &str) -> PathBuf {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("npm-package");
    loop {
        let candidate = parent.join(format!(
            ".{name}.elephc-{role}-{}-{}",
            std::process::id(),
            STAGING_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        if fs::symlink_metadata(&candidate)
            .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
        {
            return candidate;
        }
    }
}

/// Removes a file, symlink, or directory if it exists, preserving real errors
/// instead of silently leaving transaction debris.
fn remove_path_if_exists(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Converts a source stem into a valid, deterministic unscoped NPM package name.
fn npm_package_name(source_stem: &str) -> String {
    let mut name = String::with_capacity(source_stem.len());
    let mut previous_dash = false;
    for ch in source_stem.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, '-' | '_' | '.') {
            Some(ch)
        } else {
            Some('-')
        };
        if let Some(ch) = normalized {
            if ch == '-' {
                if previous_dash {
                    continue;
                }
                previous_dash = true;
            } else {
                previous_dash = false;
            }
            name.push(ch);
        }
    }
    let name = name.trim_matches(['-', '.', '_']);
    let mut name = name
        .get(..name.len().min(MAX_NPM_PACKAGE_NAME_BYTES))
        .unwrap_or(name)
        .trim_end_matches(['-', '.', '_'])
        .to_string();
    if matches!(name.as_str(), "node_modules" | "favicon.ico") {
        name.insert_str(0, "elephc-");
    }
    if name.is_empty() {
        "elephc-wasm".to_string()
    } else {
        name
    }
}

/// Renders the reusable and directly executable Node.js WASI loader.
fn loader_source(package_name: &str) -> String {
    format!(
        r#"import {{ realpathSync }} from "node:fs";
import {{ readFile }} from "node:fs/promises";
import {{ fileURLToPath }} from "node:url";
import {{ isAbsolute }} from "node:path";
import {{ WASI }} from "node:wasi";

const wasmUrl = new URL("./{WASM_FILENAME}", import.meta.url);

export class WasiOptionError extends TypeError {{
  constructor(message, code = "ERR_ELEPHC_WASI_OPTION", optionPath = "options") {{
    super(message);
    this.name = "WasiOptionError";
    this.code = code;
    this.optionPath = optionPath;
  }}
}}

export class WasiArgumentError extends WasiOptionError {{
  constructor(message, argumentIndex = null) {{
    super(
      message,
      "ERR_ELEPHC_WASI_ARGUMENT",
      argumentIndex === null ? "args" : "args[" + argumentIndex + "]",
    );
    this.name = "WasiArgumentError";
    this.argumentIndex = argumentIndex;
  }}
}}

export class WasiEnvironmentError extends WasiOptionError {{
  constructor(message, envKey = null) {{
    super(message, "ERR_ELEPHC_WASI_ENVIRONMENT", "env");
    this.name = "WasiEnvironmentError";
    this.envKey = envKey;
  }}
}}

export class WasiPreopenError extends WasiOptionError {{
  constructor(message, guestPath = null) {{
    super(message, "ERR_ELEPHC_WASI_PREOPEN", "preopens");
    this.name = "WasiPreopenError";
    this.guestPath = guestPath;
  }}
}}

/** Rejects strings that cannot be transferred losslessly through WASI. */
function validateLosslessString(value, invalid) {{
  for (let offset = 0; offset < value.length; offset += 1) {{
    const codeUnit = value.charCodeAt(offset);
    if (codeUnit === 0) {{
      throw invalid(
        "contains an embedded NUL (U+0000) code unit at offset " + offset + ".",
      );
    }}
    if (codeUnit >= 0xD800 && codeUnit <= 0xDBFF) {{
      const next = value.charCodeAt(offset + 1);
      if (!(next >= 0xDC00 && next <= 0xDFFF)) {{
        throw invalid(
          "contains an unpaired high UTF-16 surrogate code unit at offset "
            + offset + ".",
        );
      }}
      offset += 1;
    }} else if (codeUnit >= 0xDC00 && codeUnit <= 0xDFFF) {{
      throw invalid(
        "contains an unpaired low UTF-16 surrogate code unit at offset "
          + offset + ".",
      );
    }}
  }}
}}

/** Validates arguments and returns a one-read immutable snapshot. */
function validateWasiArguments(args) {{
  let isArray = false;
  try {{
    isArray = Array.isArray(args);
  }} catch {{
    // A revoked Proxy is not a stable argument container.
  }}
  if (!isArray) {{
    const received = args === null ? "null" : typeof args;
    throw new WasiArgumentError(
      "Invalid WASI arguments: expected an Array, received " + received + ".",
    );
  }}
  let length;
  let keys;
  try {{
    length = Reflect.getOwnPropertyDescriptor(args, "length").value;
    keys = Reflect.ownKeys(args);
  }} catch {{
    throw new WasiArgumentError(
      "Invalid WASI arguments: the Array could not be inspected safely.",
    );
  }}
  const snapshot = new Array(length);
  for (let index = 0; index < length; index += 1) {{
    let descriptor;
    try {{
      descriptor = Reflect.getOwnPropertyDescriptor(args, String(index));
    }} catch {{
      throw new WasiArgumentError(
        "Invalid WASI argument at index " + index
          + ": the property could not be inspected safely.",
        index,
      );
    }}
    if (!descriptor || !descriptor.enumerable || !("value" in descriptor)) {{
      throw new WasiArgumentError(
        "Invalid WASI argument at index " + index
          + ": expected a primitive string, received "
          + (descriptor ? "accessor" : "undefined") + ".",
        index,
      );
    }}
    const argument = descriptor.value;
    if (typeof argument !== "string") {{
      throw new WasiArgumentError(
        "Invalid WASI argument at index " + index
          + ": expected a primitive string, received " + typeof argument + ".",
        index,
      );
    }}
    validateLosslessString(argument, (reason) => new WasiArgumentError(
      "Invalid WASI argument at index " + index + ": argument " + reason,
      index,
    ));
    snapshot[index] = argument;
  }}
  for (const key of keys) {{
    if (key === "length") continue;
    const index = typeof key === "string" ? Number(key) : Number.NaN;
    if (!Number.isInteger(index) || index < 0 || index >= length || String(index) !== key) {{
      throw new WasiArgumentError(
        "Invalid WASI arguments: unexpected own property "
          + (typeof key === "symbol" ? "Symbol" : JSON.stringify(key)) + ".",
      );
    }}
  }}
  return Object.freeze(snapshot);
}}

/** Reads each own enumerable data property exactly once. */
function snapshotRecordEntries(value, label, allowProcessEnv, makeError) {{
  let prototype;
  let keys;
  try {{
    if (value === null || typeof value !== "object" || Array.isArray(value)) {{
      throw new TypeError();
    }}
    prototype = Reflect.getPrototypeOf(value);
    keys = Reflect.ownKeys(value);
  }} catch {{
    throw makeError("Invalid WASI " + label + ": expected a plain object.", null);
  }}
  if (
    !(allowProcessEnv && value === process.env)
    && prototype !== Object.prototype
    && prototype !== null
  ) {{
    throw makeError("Invalid WASI " + label + ": expected a plain object.", null);
  }}
  const entries = [];
  for (const key of keys) {{
    if (typeof key !== "string") {{
      throw makeError("Invalid WASI " + label + ": symbol keys are not supported.", null);
    }}
    let descriptor;
    try {{
      descriptor = Reflect.getOwnPropertyDescriptor(value, key);
    }} catch {{
      throw makeError(
        "Invalid WASI " + label + " entry " + JSON.stringify(key)
          + ": the property could not be inspected safely.",
        key,
      );
    }}
    if (!descriptor || !descriptor.enumerable || !("value" in descriptor)) {{
      throw makeError(
        "Invalid WASI " + label + " entry " + JSON.stringify(key)
          + ": expected an enumerable data property.",
        key,
      );
    }}
    entries.push([key, descriptor.value]);
  }}
  return entries;
}}

/** Validates and snapshots an explicit WASI environment. */
function validateWasiEnvironment(env) {{
  const entries = snapshotRecordEntries(
    env,
    "environment",
    true,
    (message, key) => new WasiEnvironmentError(message, key),
  );
  const snapshot = Object.create(null);
  for (const [key, value] of entries) {{
    if (key.length === 0 || key.includes("=")) {{
      throw new WasiEnvironmentError(
        "Invalid WASI environment key " + JSON.stringify(key)
          + ": keys must be non-empty and must not contain '='.",
        key,
      );
    }}
    validateLosslessString(key, (reason) => new WasiEnvironmentError(
      "Invalid WASI environment key " + JSON.stringify(key) + ": key " + reason,
      key,
    ));
    if (typeof value !== "string") {{
      throw new WasiEnvironmentError(
        "Invalid WASI environment entry " + JSON.stringify(key)
          + ": expected a primitive string value, received "
          + (value === null ? "null" : typeof value) + ".",
        key,
      );
    }}
    validateLosslessString(value, (reason) => new WasiEnvironmentError(
      "Invalid WASI environment entry " + JSON.stringify(key) + ": value " + reason,
      key,
    ));
    snapshot[key] = value;
  }}
  return Object.freeze(snapshot);
}}

/** Validates and snapshots guest-to-host preopens without rewriting paths. */
function validateWasiPreopens(preopens) {{
  const entries = snapshotRecordEntries(
    preopens,
    "preopens",
    false,
    (message, key) => new WasiPreopenError(message, key),
  );
  const snapshot = Object.create(null);
  for (const [guestPath, hostPath] of entries) {{
    validateLosslessString(guestPath, (reason) => new WasiPreopenError(
      "Invalid WASI preopen guest path " + JSON.stringify(guestPath)
        + ": path " + reason,
      guestPath,
    ));
    const segments = guestPath === "/" ? [] : guestPath.slice(1).split("/");
    if (
      !guestPath.startsWith("/")
      || (guestPath !== "/" && guestPath.endsWith("/"))
      || segments.some((segment) =>
        segment.length === 0 || segment === "." || segment === ".."
      )
    ) {{
      throw new WasiPreopenError(
        "Invalid WASI preopen guest path " + JSON.stringify(guestPath)
          + ": expected a canonical absolute POSIX path.",
        guestPath,
      );
    }}
    if (typeof hostPath !== "string") {{
      throw new WasiPreopenError(
        "Invalid WASI preopen host path for " + JSON.stringify(guestPath)
          + ": expected a primitive string, received "
          + (hostPath === null ? "null" : typeof hostPath) + ".",
        guestPath,
      );
    }}
    validateLosslessString(hostPath, (reason) => new WasiPreopenError(
      "Invalid WASI preopen host path for " + JSON.stringify(guestPath)
        + ": path " + reason,
      guestPath,
    ));
    if (!isAbsolute(hostPath)) {{
      throw new WasiPreopenError(
        "Invalid WASI preopen host path for " + JSON.stringify(guestPath)
          + ": expected an absolute host path.",
        guestPath,
      );
    }}
    snapshot[guestPath] = hostPath;
  }}
  return Object.freeze(snapshot);
}}

/** Validates top-level options before WASI or module I/O is attempted. */
function validateRunOptions(options) {{
  const entries = snapshotRecordEntries(
    options === undefined ? {{}} : options,
    "options",
    false,
    (message, key) => new WasiOptionError(
      message,
      "ERR_ELEPHC_WASI_OPTION",
      key === null ? "options" : "options." + key,
    ),
  );
  const values = Object.create(null);
  for (const [key, value] of entries) {{
    if (key !== "args" && key !== "env" && key !== "preopens") {{
      throw new WasiOptionError(
        "Invalid WASI option: unknown key " + JSON.stringify(key) + ".",
        "ERR_ELEPHC_WASI_OPTION",
        "options." + key,
      );
    }}
    values[key] = value;
  }}
  return {{
    args: validateWasiArguments(values.args === undefined ? ["{package_name}"] : values.args),
    env: validateWasiEnvironment(values.env === undefined ? {{}} : values.env),
    preopens: validateWasiPreopens(values.preopens === undefined ? {{}} : values.preopens),
  }};
}}

/**
 * Runs the compiled PHP command under Node's WASI preview1 runtime.
 *
 * @param {{ args?: readonly string[], env?: Readonly<Record<string, string | undefined>>, preopens?: Readonly<Record<string, string>> }} options
 * @returns {{Promise<number>}} the WASI process exit code
 * @throws {{WasiOptionError}} before WASI construction when an option cannot be
 * represented losslessly.
 */
export async function run(options = undefined) {{
  const {{ args, env, preopens }} = validateRunOptions(options);
  const wasi = new WASI({{
    version: "preview1",
    args,
    env,
    preopens,
    returnOnExit: true,
  }});
  const module = await WebAssembly.compile(await readFile(wasmUrl));
  const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
  const exitCode = wasi.start(instance);
  return typeof exitCode === "number" ? exitCode : 0;
}}

/** Returns false when argv[1] is absent or cannot name a real entry file. */
function isDirectInvocation(invokedPath) {{
  if (!invokedPath) return false;
  try {{
    return realpathSync(invokedPath) === realpathSync(fileURLToPath(import.meta.url));
  }} catch {{
    return false;
  }}
}}

if (isDirectInvocation(process.argv[1])) {{
  process.exitCode = await run({{
    args: [process.argv[1], ...process.argv.slice(2)],
  }});
}}
"#
    )
}

/// Returns TypeScript declarations for the generated loader API.
fn type_declarations() -> &'static str {
    r#"export declare class WasiOptionError extends TypeError {
  readonly name: string;
  readonly code: string;
  readonly optionPath: string;
  constructor(message: string, code?: string, optionPath?: string);
}

export declare class WasiArgumentError extends WasiOptionError {
  readonly name: "WasiArgumentError";
  readonly code: "ERR_ELEPHC_WASI_ARGUMENT";
  readonly argumentIndex: number | null;
  constructor(message: string, argumentIndex?: number | null);
}

export declare class WasiEnvironmentError extends WasiOptionError {
  readonly name: "WasiEnvironmentError";
  readonly code: "ERR_ELEPHC_WASI_ENVIRONMENT";
  readonly envKey: string | null;
  constructor(message: string, envKey?: string | null);
}

export declare class WasiPreopenError extends WasiOptionError {
  readonly name: "WasiPreopenError";
  readonly code: "ERR_ELEPHC_WASI_PREOPEN";
  readonly guestPath: string | null;
  constructor(message: string, guestPath?: string | null);
}

export interface RunOptions {
  args?: readonly string[];
  env?: Readonly<Record<string, string | undefined>>;
  preopens?: Readonly<Record<string, string>>;
}

export declare function run(options?: RunOptions): Promise<number>;
"#
}

/// Renders concise usage documentation for the generated package.
fn readme_source(package_name: &str, source_stem: &str) -> String {
    format!(
        r#"# {package_name}

Node.js WASI package generated by elephc from `{source_stem}.php`.

Requires Node.js 20 or newer.

```js
import {{
  run,
  WasiArgumentError,
  WasiEnvironmentError,
  WasiPreopenError,
}} from "{package_name}";

const exitCode = await run({{
  args: ["{package_name}", "first-argument"],
  env: process.env,
  preopens: {{ "/work": process.cwd() }},
}});
```

`run()` passes valid argument arrays through unchanged, including empty arrays,
empty strings, and Unicode represented by paired UTF-16 surrogates. Before
constructing WASI or loading the module, it rejects a non-array `args` value,
non-string elements, embedded NUL (`U+0000`) code units, and unpaired UTF-16
surrogates. Rejections use `WasiArgumentError` with
`code === "ERR_ELEPHC_WASI_ARGUMENT"` and `argumentIndex` set to the invalid
element index, or `null` when `args` itself is invalid.

For deterministic runs, the environment is empty unless `env` is passed
explicitly; this intentionally differs from inheriting `process.env`.
`process.env` is accepted directly despite its TypeScript value union, but any
own property whose runtime value is actually `undefined` is rejected rather
than silently filtered. Environment keys and present values must be primitive
strings; empty keys, `=`, embedded NUL, unpaired surrogates, `undefined`, and
implicit coercions are rejected with `WasiEnvironmentError`. Preopen guest
paths must be canonical absolute POSIX
paths and host paths must already be absolute on the current platform.
`WasiPreopenError` reports invalid mappings. All inputs are copied once from
own enumerable data properties before Node constructs WASI, preventing getters
or later mutation from changing the validated values.

Entries follow ECMAScript own-key order (integer-like keys first, then other
strings in insertion order). A record cannot contain duplicate keys; only the
last JavaScript assignment made before `run()` is observable. Empty environment
values and `=` inside values are valid, while `=` inside keys is not. Every
`run()` snapshots afresh without shared state. Distinct canonical guest paths
may map to the same unresolved host path.

A preopen grants the WASI program access to the mapped host path under Node's
WASI implementation. This loader neither resolves nor verifies that path and
does not provide a host filesystem sandbox; use Node's permission model and
least-privilege mappings where appropriate.

```js
try {{
  await run({{ args: ["{package_name}", "bad\0argument"] }});
}} catch (error) {{
  if (error instanceof WasiArgumentError) {{
    console.error(error.code, error.argumentIndex, error.message);
  }}
}}
```

Run the command directly:

```bash
node index.mjs first-argument
```
"#
    )
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Regression tests for the generated Node.js WASI package layout and metadata.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests use a unique temporary directory and do not require Node.js.

    use super::{npm_package_name, write_package, write_package_atomic};
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temporary package directory for a parallel test run.
    fn temp_package_dir() -> std::path::PathBuf {
        let sequence = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "elephc_npm_package_{}_{}",
            std::process::id(),
            sequence
        ))
    }

    /// Lists transaction staging and backup entries belonging to `package_dir`.
    fn package_debris(package_dir: &std::path::Path) -> Vec<String> {
        let parent = package_dir.parent().expect("package parent");
        let package_name = package_dir
            .file_name()
            .and_then(|name| name.to_str())
            .expect("package name");
        fs::read_dir(parent)
            .expect("read package parent")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                name.starts_with(&format!(".{package_name}.elephc-stage-"))
                    || name.starts_with(&format!(".{package_name}.elephc-backup-"))
            })
            .collect()
    }

    /// Verifies package names are lowercase, NPM-safe, and never empty.
    #[test]
    fn normalizes_npm_package_names() {
        assert_eq!(npm_package_name("Hello_WASM"), "hello_wasm");
        assert_eq!(npm_package_name("hello world!"), "hello-world");
        assert_eq!(npm_package_name("..."), "elephc-wasm");
        assert_eq!(npm_package_name("node_modules"), "elephc-node_modules");
        assert_eq!(npm_package_name(&"a".repeat(300)).len(), 214);
    }

    /// Verifies `--emit npm` writes the binary, ESM loader, types, metadata, and README.
    #[test]
    fn writes_complete_npm_package() {
        let package_dir = temp_package_dir();
        let wasm = b"\0asm\x01\0\0\0";
        write_package(&package_dir, "Hello App", wasm).expect("write npm package");

        assert_eq!(
            fs::read(package_dir.join("module.wasm")).expect("read module"),
            wasm
        );
        let metadata: serde_json::Value = serde_json::from_slice(
            &fs::read(package_dir.join("package.json")).expect("read package metadata"),
        )
        .expect("parse package metadata");
        assert_eq!(metadata["name"], "hello-app");
        assert_eq!(metadata["type"], "module");
        assert_eq!(metadata["exports"]["."]["types"], "./index.d.ts");
        assert_eq!(metadata["exports"]["."]["import"], "./index.mjs");
        assert_eq!(metadata["exports"]["."]["default"], "./index.mjs");

        let loader = fs::read_to_string(package_dir.join("index.mjs")).expect("read loader");
        assert!(loader.contains("new WASI"));
        assert!(loader.contains("version: \"preview1\""));
        assert!(loader.contains("wasi.getImportObject()"));
        assert!(loader.contains("typeof value !== \"string\""));
        assert!(loader.contains("export async function run"));
        assert!(loader.contains("export class WasiOptionError extends TypeError"));
        assert!(loader.contains("export class WasiArgumentError extends WasiOptionError"));
        assert!(loader.contains("export class WasiEnvironmentError extends WasiOptionError"));
        assert!(loader.contains("export class WasiPreopenError extends WasiOptionError"));
        assert!(loader.contains("\"ERR_ELEPHC_WASI_ARGUMENT\""));
        assert!(loader.contains("\"ERR_ELEPHC_WASI_ENVIRONMENT\""));
        assert!(loader.contains("\"ERR_ELEPHC_WASI_PREOPEN\""));
        assert!(loader.contains("this.argumentIndex = argumentIndex"));
        assert!(loader.contains("function validateWasiArguments(args)"));
        assert!(loader.contains("function validateWasiEnvironment(env)"));
        assert!(loader.contains("function validateWasiPreopens(preopens)"));
        assert!(loader.contains("function validateRunOptions(options)"));
        assert!(loader.contains("function isDirectInvocation(invokedPath)"));
        assert!(loader.contains("catch {\n    return false;"));
        assert!(loader.contains("Reflect.getOwnPropertyDescriptor"));
        assert!(loader.contains("Object.freeze(snapshot)"));
        assert!(loader.contains("isAbsolute(hostPath)"));
        assert!(loader.contains("codeUnit === 0"));
        assert!(loader.contains("codeUnit >= 0xD800 && codeUnit <= 0xDBFF"));
        assert!(loader.contains("next >= 0xDC00 && next <= 0xDFFF"));
        let validation = loader
            .find("validateRunOptions(options);")
            .expect("option validation call");
        let wasi_construction = loader.find("new WASI").expect("WASI construction");
        let module_compilation = loader
            .find("WebAssembly.compile")
            .expect("WebAssembly compilation");
        assert!(
            validation < wasi_construction && validation < module_compilation,
            "options must be validated before any WASI or WebAssembly work"
        );
        let declarations =
            fs::read_to_string(package_dir.join("index.d.ts")).expect("read declarations");
        assert!(declarations.contains("string | undefined"));
        assert!(declarations.contains("args?: readonly string[]"));
        assert!(declarations.contains(
            "env?: Readonly<Record<string, string | undefined>>"
        ));
        assert!(declarations.contains("preopens?: Readonly<Record<string, string>>"));
        assert!(declarations.contains("export declare class WasiOptionError extends TypeError"));
        assert!(declarations.contains(
            "export declare class WasiArgumentError extends WasiOptionError"
        ));
        assert!(declarations.contains("export declare class WasiEnvironmentError"));
        assert!(declarations.contains("export declare class WasiPreopenError"));
        assert!(declarations.contains("readonly name: \"WasiArgumentError\""));
        assert!(declarations.contains("readonly code: \"ERR_ELEPHC_WASI_ARGUMENT\""));
        assert!(declarations.contains("readonly argumentIndex: number | null"));
        assert!(declarations.contains(
            "constructor(message: string, argumentIndex?: number | null)"
        ));
        assert!(package_dir.join("index.d.ts").is_file());
        let readme =
            fs::read_to_string(package_dir.join("README.md")).expect("read package README");
        assert!(readme.contains("WasiArgumentError"));
        assert!(readme.contains("ERR_ELEPHC_WASI_ARGUMENT"));
        assert!(readme.contains("embedded NUL (`U+0000`)"));
        assert!(readme.contains("unpaired UTF-16"));
        assert!(readme.contains("environment is empty unless `env` is passed"));
        assert!(readme.contains("intentionally differs from inheriting `process.env`"));
        assert!(readme.contains("runtime value is actually `undefined`"));
        assert!(readme.contains("does not provide a host filesystem sandbox"));

        fs::remove_dir_all(package_dir).expect("remove temporary package");
    }

    /// Verifies replacing an existing package preserves a complete final tree
    /// and leaves no staging or backup directory behind.
    #[test]
    fn atomically_replaces_existing_package() {
        let package_dir = temp_package_dir();
        write_package(&package_dir, "Old App", b"old").expect("write old package");

        write_package_atomic(&package_dir, "New App", b"new").expect("replace package");

        assert_eq!(
            fs::read(package_dir.join("module.wasm")).expect("read replaced module"),
            b"new"
        );
        let debris = package_debris(&package_dir);
        assert!(debris.is_empty(), "unexpected package debris: {debris:?}");

        fs::remove_dir_all(package_dir).expect("remove temporary package");
    }

    /// Verifies an existing non-directory destination is rejected without
    /// renaming or deleting the user's file or leaving transaction debris.
    #[test]
    fn rejects_existing_file_destination_without_mutation() {
        let package_dir = temp_package_dir();
        fs::write(&package_dir, b"user-owned").expect("write destination file");

        let error = write_package_atomic(&package_dir, "New App", b"new")
            .expect_err("file destination must be rejected");

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(
            error.to_string().contains("is not a directory"),
            "unexpected error: {error}"
        );
        assert_eq!(
            fs::read(&package_dir).expect("read destination file"),
            b"user-owned",
            "the existing file must remain byte-identical"
        );
        let debris = package_debris(&package_dir);
        assert!(debris.is_empty(), "unexpected package debris: {debris:?}");

        fs::remove_file(package_dir).expect("remove destination file");
    }
}
