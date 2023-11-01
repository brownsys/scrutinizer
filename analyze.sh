#!/bin/bash

export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir -Znll-facts"
cargo install --locked --path ./purifier/ && cd $1 && cargo clean && cargo purifier --function $2
