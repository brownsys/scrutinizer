#!/bin/bash

export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir"
export RUST_BACKTRACE=full
export RUST_LOG=scrutinizer=trace

cd $1 && cargo clean && cargo scrutinizer --only-inconsistent --out-file inconsistent.result.json
