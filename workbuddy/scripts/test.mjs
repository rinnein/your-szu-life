#!/usr/bin/env node
// Run the WorkBuddy Node test suite with the same Node runtime as vpx.

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const testDirectory = fileURLToPath(new URL("../test", import.meta.url));
const child = spawn(process.execPath, ["--test", testDirectory], {
  stdio: "inherit",
  windowsHide: true,
});
child.on("exit", (code) => {
  process.exit(code ?? 1);
});
