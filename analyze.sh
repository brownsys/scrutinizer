#!/bin/bash

export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir"
cargo install --locked --path ./purifier/ && cd $1 && cargo clean && cargo purifier --function $2
