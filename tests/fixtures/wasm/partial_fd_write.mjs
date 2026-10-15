import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const modulePath = process.argv[2];
if (!modulePath) {
  throw new Error("usage: node partial_fd_write.mjs <module.wasm>");
}

const args = ["host-portability", "first"];
const encodedArgs = args.map((arg) => new TextEncoder().encode(`${arg}\0`));
const expectedStdout = Buffer.from("2|first\n");
const maxWriteSize = 2;

let memory;
const stdout = [];
const stderr = [];

class WasiExit extends Error {
  constructor(status) {
    super(`WASI process exited with status ${status}`);
    this.status = status;
  }
}

function memoryView() {
  assert.ok(memory instanceof WebAssembly.Memory, "module must export its memory");
  return new DataView(memory.buffer);
}

function copyToGuest(address, bytes) {
  new Uint8Array(memory.buffer, address, bytes.length).set(bytes);
}

const wasi = {
  proc_exit(status) {
    throw new WasiExit(status);
  },

  fd_write(fd, iovecsAddress, iovecsLength, writtenAddress) {
    const view = memoryView();
    let remaining = maxWriteSize;
    let written = 0;

    for (let index = 0; index < iovecsLength && remaining > 0; index += 1) {
      const iovecAddress = iovecsAddress + index * 8;
      const bufferAddress = view.getUint32(iovecAddress, true);
      const bufferLength = view.getUint32(iovecAddress + 4, true);
      const length = Math.min(bufferLength, remaining);
      if (length === 0) {
        continue;
      }

      const bytes = Buffer.from(
        new Uint8Array(memory.buffer, bufferAddress, length),
      );
      if (fd === 1) {
        stdout.push(bytes);
      } else if (fd === 2) {
        stderr.push(bytes);
      } else {
        return 8;
      }
      written += length;
      remaining -= length;
    }

    view.setUint32(writtenAddress, written, true);
    return 0;
  },

  args_sizes_get(argcAddress, bufferSizeAddress) {
    const view = memoryView();
    const bufferSize = encodedArgs.reduce((total, arg) => total + arg.length, 0);
    view.setUint32(argcAddress, encodedArgs.length, true);
    view.setUint32(bufferSizeAddress, bufferSize, true);
    return 0;
  },

  args_get(argvAddress, bufferAddress) {
    const view = memoryView();
    let cursor = bufferAddress;
    for (const [index, arg] of encodedArgs.entries()) {
      view.setUint32(argvAddress + index * 4, cursor, true);
      copyToGuest(cursor, arg);
      cursor += arg.length;
    }
    return 0;
  },
};

const { instance } = await WebAssembly.instantiate(await readFile(modulePath), {
  wasi_snapshot_preview1: wasi,
});
memory = instance.exports.memory;

let exitStatus = 0;
try {
  instance.exports._start();
} catch (error) {
  if (!(error instanceof WasiExit)) {
    throw error;
  }
  exitStatus = error.status;
}

assert.equal(exitStatus, 7);
assert.deepEqual(Buffer.concat(stdout), expectedStdout);
assert.deepEqual(Buffer.concat(stderr), Buffer.alloc(0));
