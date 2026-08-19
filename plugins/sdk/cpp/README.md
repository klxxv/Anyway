# MYC C++ SDK

Compile plugins to freestanding WebAssembly. The VM provides no imports, filesystem,
network, clock, environment, or application-store access.

```bash
clang++ --target=wasm32 -std=c++20 -O2 -nostdlib \
  -Wl,--no-entry -Wl,--export=memory \
  -Wl,--export=myc_alloc -Wl,--export=myc_run \
  -Wl,--initial-memory=131072 -Wl,--max-memory=16777216 \
  example.cpp -o plugin.wasm
```

Package `plugin.wasm` beside a runtime `plugin.json` whose engine is `wasm32-myc`,
language is `cpp`, capability is `analysis.run`, and permissions are empty.
