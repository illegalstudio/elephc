import {
  run,
  WasiArgumentError,
  WasiEnvironmentError,
  WasiPreopenError,
} from "host_portability";

declare const process: {
  env: Readonly<Record<string, string | undefined>>;
};

const args: readonly string[] = ["host-portability", "first"];
const exitCode: Promise<number> = run({
  args,
  env: process.env,
  preopens: {},
});

async function handlesInvalidArguments(): Promise<void> {
  try {
    await run({ args: ["host-portability", "bad\0argument"] });
  } catch (error: unknown) {
    if (error instanceof WasiArgumentError) {
      const name: "WasiArgumentError" = error.name;
      const code: "ERR_ELEPHC_WASI_ARGUMENT" = error.code;
      const argumentIndex: number | null = error.argumentIndex;
      void name;
      void code;
      void argumentIndex;
    }
  }
}

void exitCode;
void handlesInvalidArguments;
void WasiEnvironmentError;
void WasiPreopenError;
