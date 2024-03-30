# scrutinizer

All necessary scripts to run `scrutinizer` are in `scripts` directory.

Before running `scrutinizer` on any crate, you should recompile the crate (and its dependencies) with `-Znll-facts` flag set because taint-tracking uses `flowistry`, which depends on using `polonius` borrow checker facts. `scripts/generate_facts.sh` sets all necessary flags and recompiles the crate. You need to do it every time the code inside the crate changes. It is quite slow (~5 minutes for `websubmit`), but I think it's possible to figure out how to recompile only things that have changed between compilations.

Among other caveats, we must tell cargo to build `std` every time we run `scrutinizer`. See `test-crate/.cargo/config.toml` for an example of a configuration that achieves that. 

Finally, you can build `scrutinizer` using `./scripts/build.sh` and run it using `./scripts/analyze.sh $DIR $CONFIG`, where `$DIR` is the crate you want to analyze, and `$CONFIG` is the path to the config file inside the directory. The run script sets all necessary flags, such as `-Zalways-encode-mir`, which `scrutinizer` depends on and controls logging and backtrace output.

We provide an example of `scrutinizer` configuration at `test-crate/sample-scrutinizer-config.toml`.
