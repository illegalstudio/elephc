/**
 * Runs one generated Elephc npm package without contaminating PHP output and
 * publishes the complete WASI i32 exit bit pattern on a dedicated file
 * descriptor owned by the differential-oracle parent process.
 */

import { closeSync, readFileSync, writeSync } from "node:fs";
import { isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";

const CONFIG_SCHEMA = "elephc.wasm-oracle.node-run.v1";
const STATUS_FD_ENV = "ELEPHC_ORACLE_MODULE_STATUS_FD";

/** Rejects values that are not ordinary JSON objects. */
function requirePlainObject(value, label) {
  if (
    value === null
    || typeof value !== "object"
    || Array.isArray(value)
    || Object.getPrototypeOf(value) !== Object.prototype
  ) {
    throw new TypeError(`${label} must be a plain JSON object`);
  }
}

/** Rejects values whose JSON object shape is not exact. */
function requireExactKeys(value, expected, label) {
  requirePlainObject(value, label);
  const actual = Object.keys(value);
  if (
    actual.length !== expected.length
    || actual.some((key, index) => key !== expected[index])
  ) {
    throw new TypeError(
      `${label} keys must be exactly ${JSON.stringify(expected)} in canonical order`,
    );
  }
}

/** Validates a logical guest environment or preopen string record. */
function requireStringRecord(value, label) {
  requirePlainObject(value, label);
  for (const [key, entry] of Object.entries(value)) {
    if (key.length === 0 || key.includes("\0")) {
      throw new TypeError(`${label} contains an invalid key`);
    }
    if (typeof entry !== "string" || entry.includes("\0")) {
      throw new TypeError(`${label} ${JSON.stringify(key)} must be a string`);
    }
  }
}

/** Loads canonical JSON so duplicate keys, alternate ordering, and whitespace fail. */
function loadCanonicalConfig(path) {
  const source = readFileSync(path, "utf8");
  const config = JSON.parse(source);
  requireExactKeys(
    config,
    ["args", "env", "preopens", "program", "schema"],
    "oracle config",
  );
  if (`${JSON.stringify(config)}\n` !== source) {
    throw new TypeError("oracle config must use canonical single-line JSON");
  }
  if (config.schema !== CONFIG_SCHEMA) {
    throw new TypeError(
      `unsupported oracle config schema ${JSON.stringify(config.schema)}`,
    );
  }
  if (typeof config.program !== "string" || config.program.length === 0) {
    throw new TypeError("oracle config program must be a non-empty string");
  }
  if (
    !Array.isArray(config.args)
    || config.args.some((argument) => typeof argument !== "string")
  ) {
    throw new TypeError("oracle config args must contain only strings");
  }
  requireStringRecord(config.env, "oracle guest env");
  requireStringRecord(config.preopens, "oracle guest preopens");
  return config;
}

/** Parses the inherited control descriptor without exposing it to the guest. */
function statusDescriptor() {
  const raw = process.env[STATUS_FD_ENV];
  if (!/^[3-9][0-9]*$/.test(raw ?? "")) {
    throw new TypeError(`${STATUS_FD_ENV} must name an inherited descriptor >= 3`);
  }
  return Number(raw);
}

/** Converts a WASI result to its exact unsigned 32-bit representation. */
function statusBits(status) {
  if (
    typeof status !== "number"
    || !Number.isSafeInteger(status)
    || status < -0x8000_0000
    || status > 0xffff_ffff
  ) {
    throw new TypeError(`WASI exit status is not representable as i32: ${status}`);
  }
  return Number(BigInt.asUintN(32, BigInt(status)));
}

/** Writes the complete status frame, retrying partial descriptor writes. */
function publishStatus(descriptor, bits) {
  const payload = Buffer.from(`${bits.toString(16).padStart(8, "0")}\n`, "ascii");
  let offset = 0;
  try {
    while (offset < payload.length) {
      const written = writeSync(
        descriptor,
        payload,
        offset,
        payload.length - offset,
      );
      if (written === 0) {
        throw new Error("status descriptor made zero write progress");
      }
      offset += written;
    }
  } finally {
    closeSync(descriptor);
  }
}

/** Executes one package and maps the host process status to the POSIX low byte. */
async function main() {
  if (process.argv.length !== 4) {
    throw new TypeError(
      "usage: node_runner.mjs <absolute-index.mjs> <canonical-config.json>",
    );
  }
  const loaderPath = process.argv[2];
  const configPath = process.argv[3];
  if (!isAbsolute(loaderPath) || !isAbsolute(configPath)) {
    throw new TypeError("loader and config paths must be absolute");
  }
  const config = loadCanonicalConfig(configPath);
  const descriptor = statusDescriptor();
  delete process.env[STATUS_FD_ENV];

  const { run } = await import(pathToFileURL(loaderPath));
  if (typeof run !== "function") {
    throw new TypeError("generated npm loader does not export run()");
  }
  const bits = statusBits(await run({
    args: [config.program, ...config.args],
    env: config.env,
    preopens: config.preopens,
  }));
  publishStatus(descriptor, bits);
  process.exitCode = bits & 0xff;
}

await main();
