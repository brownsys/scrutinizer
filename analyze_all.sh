#!/bin/bash

cargo install --locked --path ./scrutinizer/
export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir -Znll-facts"
export RUST_BACKTRACE=full
cd $1 && cargo clean && cargo scrutinizer --out-file $2
