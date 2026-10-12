//! `--screenshot`: capture the window once it has settled, write a PNG, exit.
use crate::Message;
use iced::Task;

/// Long enough for fonts, diagrams and images to load.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(3);

pub fn schedule() -> Task<Message> {
    Task::perform(tokio::time::sleep(SETTLE), |_| Message::Capture)
}
pub fn capture() -> Task<Message> {
    iced::window::latest()
        .and_then(iced::window::screenshot)
        .map(Message::Captured)
}
pub fn save(path: &str, shot: &iced::window::Screenshot) {
    let file = std::fs::File::create(path).expect("screenshot output");
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        shot.size.width,
        shot.size.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("PNG header")
        .write_image_data(&shot.rgba)
        .expect("PNG data");
    println!("Screenshot saved: {path}");
}
