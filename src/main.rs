mod analysis;
mod app;
mod audio;
pub mod model;
mod persistence;
mod ui;

use app::OtomieruApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "OtoMieru Boyer",
        options,
        Box::new(|cc| Ok(Box::new(OtomieruApp::new(cc)))),
    )
}
