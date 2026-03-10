pub mod app;
pub mod key_binds;
pub mod localize;
pub mod menu;
pub mod message;
pub mod views;
pub mod watcher;

use app::CosmicViewer;
use std::path::PathBuf;

fn main() -> cosmic::iced::Result {
    let settings = cosmic::app::Settings::default();

    // Get the image if opened from the file manager or cli
    let mut optional_image = std::env::args().nth(1).map(PathBuf::from);

    // Make /home/$USER/Pictures the default directory to open to.
    if optional_image.is_none() {
        optional_image = Some(
            std::env::var("HOME")
                .map(PathBuf::from)
                .expect("/home/$USER should exist")
                .join("Pictures"),
        );
    }

    cosmic::app::run::<CosmicViewer>(settings, optional_image)
}
