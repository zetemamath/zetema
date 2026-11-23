# Changelog

## 0.1.0 - 2026-11-23

The first release, with the lab's first record.

- Record 001: C(83,17,2) = 30. The best covering known before had 31 blocks and dated from 2011, and no covering of these sizes can have fewer than 30 blocks (the Schönheim bound), so the entry is closed. The certificate and its OpenTimestamps proof, anchored in Bitcoin block 977177, are attached.
- `zetema check` checks certificates exactly, one bit per t-set. Its exit code is 0 only when every file given is a covering.
- `zetema search` runs a weighted local search for t = 2 and t = 3 that repeats exactly from its seed. It carries the first change the lab adopted from an AI model, the `walk` setting.
- `zetema hunt` runs many searches side by side and reports each as one line of JSON. `--near` keeps the nearest misses, and `--status` writes how far every running search has come about once a second.
- `zetema calldata` turns a certificate into the call that submits it to the registry.
- `zetema.wasm` is the checker the site runs, as a WebAssembly module with three plain exports.
- `contracts/` holds the covering registry, the bounty board and the sponsor contract, with their Foundry tests. Addresses follow at deployment.
