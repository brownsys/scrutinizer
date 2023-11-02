#!/bin/bash

cargo install --locked --path ./vartrack/
export RUSTFLAGS="$RUSTFLAGS -Zalways-encode-mir -Znll-facts"
cd $1 && cargo clean && cargo vartrack --function $2
