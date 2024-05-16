if [ "$#" -ne 1 ]; then
    echo "usage: ./run_lint_verbose test_suite" >&2
    exit 2
fi

cd $1 &&
export RUSTFLAGS="-Z always-encode-mir -Z nll-facts" &&
cargo +nightly-2023-04-12 dylint --all -- -Z build-std --target aarch64-apple-darwin
