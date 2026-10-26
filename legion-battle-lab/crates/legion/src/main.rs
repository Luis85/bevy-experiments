mod app;
mod camera;
mod crowd;
mod runtime;
mod smoke;
mod ui;
mod world;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("Legion konnte nicht gestartet werden: {error}");
        std::process::exit(1);
    }
}
