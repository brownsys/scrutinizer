#!/bin/bash

cargo install --locked --path ./scrutinizer/
export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir -Znll-facts"
export RUST_BACKTRACE=full
export RUST_LOG=scrutinizer=trace

cd $1 && cargo clean && cargo scrutinizer --function $2 --important-args $3 --out-file $4
