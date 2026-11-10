// Unit tests run in Node, where the WebAssembly module is read from disk instead of fetched.

import { readFileSync } from "node:fs";

import { initSync } from "../wasm/snappy_wasm";

initSync({ module: readFileSync(new URL("../wasm/snappy_wasm_bg.wasm", import.meta.url)) });
