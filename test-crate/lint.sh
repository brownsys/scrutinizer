export RUSTFLAGS="-Zalways-encode-mir -Znll-facts" &&
cargo dylint --all -- -Z build-std --target aarch64-apple-darwin > dylint.log
