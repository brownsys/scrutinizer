fn main() {
    env_logger::init();
    rustc_plugin::cli_main(pure_func::PureFuncPlugin);
}