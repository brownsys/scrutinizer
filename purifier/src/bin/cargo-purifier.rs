fn main() {
    env_logger::init();
    rustc_plugin::cli_main(purifier::PurifierPlugin);
}