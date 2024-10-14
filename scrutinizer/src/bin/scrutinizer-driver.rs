use env_logger::Target;
use std::fs::OpenOptions;

fn main() {
    env_logger::builder()
        .target(Target::Pipe(Box::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("scrutinizer.log")
                .unwrap(),
        )))
        .init();
    rustc_plugin::driver_main(scrutinizer::ScrutinizerPlugin);
}
