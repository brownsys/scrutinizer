#!/bin/bash

export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir -Znll-facts"
cargo install --locked --path ./vartrack/ && cd $1 && cargo clean && cargo vartrack --function $2
