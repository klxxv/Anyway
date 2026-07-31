# MYC Rust SDK

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

Copy the resulting `.wasm` to `plugin.wasm`. Runtime packages declare
`engine: wasm32-myc`, `language: rust`, `analysis.run`, and no permissions.
The host supplies no imports; JSON is exchanged only through exported guest memory.
