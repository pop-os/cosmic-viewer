// SPDX-License-Identifier: GPL-3.0-only

pub mod app;
pub mod key_binds;
pub mod localize;
pub mod menu;
pub mod message;
pub mod views;
pub mod watcher;

use app::CosmicViewer;
use cosmic::{
    app::{Settings, run},
    iced::Limits,
};
use std::{
    env::{args, var},
    path::PathBuf,
};

fn main() -> cosmic::iced::Result {
    let settings = Settings::default()
        .exit_on_close(false)
        .size_limits(Limits::NONE.min_width(360.0).min_height(300.0));

    // Get the image if opened from the file manager or cli
    let mut optional_image = args().nth(1).map(PathBuf::from);

    // Make /home/$USER/Pictures the default directory to open to.
    if optional_image.is_none() {
        optional_image = Some(
            var("HOME")
                .map(PathBuf::from)
                .expect("/home/$USER should exist")
                .join("Pictures"),
        );
    }

    run::<CosmicViewer>(settings, optional_image)
}
