import { spawnSync } from "node:child_process";

const source = "plugins/sdk/cpp/example.cpp";
const candidates = process.platform === "win32" ? ["g++", "clang++"] : ["clang++", "g++"];

for (const compiler of candidates) {
  const result = spawnSync(
    compiler,
    ["-std=c++17", "-Wall", "-Wextra", "-Werror", "-fsyntax-only", source],
    { encoding: "utf8", shell: process.platform === "win32" },
  );
  if (result.error?.code === "ENOENT" || result.status === 9009) continue;
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  process.exit(result.status ?? 1);
}

console.warn("C++ SDK syntax check skipped: install clang++ or g++ to enable it.");
