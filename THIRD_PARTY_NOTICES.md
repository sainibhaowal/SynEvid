# Third-Party Software Notices & Acknowledgements

This document provides third-party license notices, copyright acknowledgements, and permissions for open-source software incorporated into, linked with, or distributed alongside **SynEvid (Code Autopsy)**.

SynEvid is licensed under the [Apache License, Version 2.0](LICENSE). We express our gratitude to the authors and maintainers of the following open-source projects for their invaluable contributions to the systems software and compiler ecosystem.

---

## 1. Direct Dependency Inventory & Licenses

### Rust Core & Verification Crates (`crates/*`)

| Component | License | Author / Copyright Holder | Repository / Source |
| :--- | :--- | :--- | :--- |
| **`blake3`** | CC0-1.0 / Apache-2.0 | Jack O'Connor, Samuel Neves, Zooko Wilcox-O'Hearn | [github.com/BLAKE3-team/BLAKE3](https://github.com/BLAKE3-team/BLAKE3) |
| **`clap`** | MIT / Apache-2.0 | Kevin K. and Clap Contributors | [github.com/clap-rs/clap](https://github.com/clap-rs/clap) |
| **`flate2`** | MIT / Apache-2.0 | Alex Crichton and Flate2 Contributors | [github.com/rust-lang/flate2-rs](https://github.com/rust-lang/flate2-rs) |
| **`glob`** | MIT / Apache-2.0 | The Rust Project Developers | [github.com/rust-lang/glob](https://github.com/rust-lang/glob) |
| **`ignore`** | MIT / Unlicense | Andrew Gallant (BurntSushi) | [github.com/BurntSushi/ripgrep/tree/master/crates/ignore](https://github.com/BurntSushi/ripgrep) |
| **`petgraph`** | MIT / Apache-2.0 | Ulrik Sverdrup, bluss, and Petgraph Contributors | [github.com/petgraph/petgraph](https://github.com/petgraph/petgraph) |
| **`rusqlite`** | MIT | John Gallagher and Rusqlite Contributors | [github.com/rusqlite/rusqlite](https://github.com/rusqlite/rusqlite) |
| **`serde`** | MIT / Apache-2.0 | Erick Tryzelaar, David Tolnay, and Serde Contributors | [github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| **`serde_json`** | MIT / Apache-2.0 | Erick Tryzelaar, David Tolnay, and Serde Contributors | [github.com/serde-rs/json](https://github.com/serde-rs/json) |
| **`serde_yaml`** | MIT / Apache-2.0 | David Tolnay | [github.com/dtolnay/serde-yaml](https://github.com/dtolnay/serde-yaml) |
| **`sha1`** | MIT / Apache-2.0 | RustCrypto Developers | [github.com/RustCrypto/hashes](https://github.com/RustCrypto/hashes) |
| **`thiserror`** | MIT / Apache-2.0 | David Tolnay | [github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| **`toml`** | MIT / Apache-2.0 | Alex Crichton, Ed Page, and TOML Contributors | [github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| **`tree-sitter`** | MIT | Max Brunsfeld and Tree-sitter Contributors | [github.com/tree-sitter/tree-sitter](https://github.com/tree-sitter/tree-sitter) |
| **`tree-sitter-typescript`** | MIT | Max Brunsfeld and Tree-sitter Contributors | [github.com/tree-sitter/tree-sitter-typescript](https://github.com/tree-sitter/tree-sitter-typescript) |
| **`tree-sitter-python`** | MIT | Max Brunsfeld and Tree-sitter Contributors | [github.com/tree-sitter/tree-sitter-python](https://github.com/tree-sitter/tree-sitter-python) |
| **`tree-sitter-go`** | MIT | Max Brunsfeld and Tree-sitter Contributors | [github.com/tree-sitter/tree-sitter-go](https://github.com/tree-sitter/tree-sitter-go) |
| **`tree-sitter-rust`** | MIT | Max Brunsfeld and Tree-sitter Contributors | [github.com/tree-sitter/tree-sitter-rust](https://github.com/tree-sitter/tree-sitter-rust) |
| **`SQLite`** (via `rusqlite`) | Public Domain | D. Richard Hipp and SQLite Contributors | [sqlite.org](https://www.sqlite.org/) |

### Model Context Protocol Server (`apps/mcp-server`)

| Component | License | Author / Copyright Holder | Repository / Source |
| :--- | :--- | :--- | :--- |
| **`@modelcontextprotocol/server`** (v2) | Apache-2.0 | Anthropic, PBC and Contributors | [github.com/modelcontextprotocol/typescript-sdk](https://github.com/modelcontextprotocol/typescript-sdk) |
| **`zod`** | MIT | Colin McDonnell and Zod Contributors | [github.com/colinhacks/zod](https://github.com/colinhacks/zod) |

---

## 2. Full License Texts

### The MIT License (MIT)

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

### Apache License, Version 2.0

```
Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

---

### Creative Commons Zero v1.0 Universal (CC0-1.0)

```
The person who associated a work with this deed has dedicated the work to the
public domain by waiving all of his or her rights to the work worldwide under
copyright law, including all related and neighboring rights, to the extent
allowed by law.

You can copy, modify, distribute and perform the work, even for commercial
purposes, all without asking permission.
```

---

### SQLite Public Domain Dedication

```
SQLite is in the Public Domain. All of the code and documentation in SQLite has
been dedicated to the public domain by the authors. All code authors, and
representatives of the companies they work for, have signed affidavits dedicating
their contributions to the public domain and originals of those signed affidavits
are stored in a firesafe at the main offices of Hwaci. Anyone is free to copy,
modify, publish, use, compile, sell, or distribute the original SQLite code,
either in source code form or as a compiled binary, for any purpose, commercial
or non-commercial, and by any means.
```
