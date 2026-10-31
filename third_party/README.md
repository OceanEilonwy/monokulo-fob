# Vendored crates

`monero-seed` and `polyseed` are copied from
[kayabaNerve/monero-wallet-util](https://github.com/kayabaNerve/monero-wallet-util)
at commit `797fe70` (2025-08-26), MIT licensed (see each `LICENSE`).

They are vendored because the crates.io `monero-seed` 0.0.1 is an empty
placeholder, the crates.io `polyseed` is a different crate, and upstream
depends on `std-shims` from serai's unpinned git branch. The only changes are in
each `Cargo.toml`: `std-shims` (and the `monero-primitives` dev-dependency) come
from crates.io, and the `[lints] workspace = true` table is removed.
