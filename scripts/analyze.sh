#!/bin/bash

export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir"
export RUST_BACKTRACE=full
export RUST_LOG=scrutinizer=trace

cd $1 && cargo clean && cargo scrutinizer --config-path=$2
