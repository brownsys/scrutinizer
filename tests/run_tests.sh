export RUSTFLAGS="-Z always-encode-mir -Z nll-facts" &&
cargo +nightly-2023-04-12 dylint -q --all -- -Z build-std --target aarch64-apple-darwin
