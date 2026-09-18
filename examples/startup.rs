use std::time::Instant;

use line::{app::App, config::ConfigStore, ui};
use ratatui::{Terminal, backend::TestBackend};

fn main() {
    let started = Instant::now();
    let store = ConfigStore::in_home().expect("config store");
    let data = store.load().expect("profiles");
    let loaded = started.elapsed();

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut app = App::new(data.profiles);
    let initialized = started.elapsed();

    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("first frame");
    let first_frame = started.elapsed();

    println!("load_config: {loaded:?}");
    println!("app_init:    {initialized:?}");
    println!("first_frame: {first_frame:?}");
}
