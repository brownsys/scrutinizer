#!/bin/bash

cargo install --locked --path ./scrutinizer/
export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir -Znll-facts"
cd $1 && cargo clean && cargo scrutinizer --function $2
