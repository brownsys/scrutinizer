export RUSTFLAGS="-Z always-encode-mir -Z nll-facts" &&
cargo +nightly-2023-04-12 dylint --all -- -Z build-std --target aarch64-apple-darwin
