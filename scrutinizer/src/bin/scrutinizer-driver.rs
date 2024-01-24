use env_logger::Target;
use std::fs::File;

fn main() {
    env_logger::builder()
        .target(Target::Pipe(Box::new(
            File::create("scrutinizer.log").unwrap(),
        )))
        .init();
    rustc_plugin::driver_main(scrutinizer::ScrutinizerPlugin);
}
