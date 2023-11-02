#!/bin/bash

cargo install --locked --path ./purifier/
export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir -Znll-facts"
cd $1 && cargo clean && cargo purifier --function $2
