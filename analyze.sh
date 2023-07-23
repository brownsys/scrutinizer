#!/bin/bash

export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir"
cargo install --path . && cd $1 && cargo clean && cargo pure-func --function $2
