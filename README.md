# Scrutinizer 

Scrutinizer is a Rust function non-leakage analyzer.

You can build and install Scrutinizer via `scripts/scrutinizer-install` and run it via `scripts/scrutinizer-run $DIR $CONFIG`, where `$DIR` is the path to the crate directory you want to analyze, and `$CONFIG` is the path to the config file **inside** the crate directory.

We provide an example of a configuration file at `test-crate/scrutinizer-config.toml`.
