// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    cosmic_config::{self, Config, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry},
    theme,
};
use serde::{Deserialize, Serialize};
use std::fmt;

const APP_ID: &str = "com.system76.CosmicViewer";
pub const COSMIC_THEME_DARK: &str = "COSMIC Dark";
pub const COSMIC_THEME_LIGHT: &str = "COSMIC Light";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WallpaperBehavior {
    #[default]
    Ask,
    AllDisplays,
    CurrentDisplay,
}

impl WallpaperBehavior {
    pub const ALL: &'static [Self] = &[Self::Ask, Self::AllDisplays, Self::CurrentDisplay];
}

impl fmt::Display for WallpaperBehavior {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ask => write!(f, "Always Ask"),
            Self::AllDisplays => write!(f, "All Displays"),
            Self::CurrentDisplay => write!(f, "Current Display"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortMode {
    #[default]
    Name,
    Date,
    Size,
}

impl SortMode {
    pub const ALL: &'static [Self] = &[Self::Name, Self::Date, Self::Size];
}

impl fmt::Display for SortMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name => write!(f, "Name"),
            Self::Date => write!(f, "Date"),
            Self::Size => write!(f, "Size"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

impl SortOrder {
    pub const ALL: &'static [Self] = &[Self::Ascending, Self::Descending];

    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

impl fmt::Display for SortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ascending => write!(f, "Ascending"),
            Self::Descending => write!(f, "Descending"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThumbnailSize {
    Small,
    Medium,
    Large,
    #[default]
    XLarge,
}

impl ThumbnailSize {
    #[must_use]
    pub const fn pixels(self) -> u32 {
        match self {
            Self::Small => 64,
            Self::Medium => 128,
            Self::Large => 192,
            Self::XLarge => 256,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum AppTheme {
    Dark,
    Light,
    System,
}

impl AppTheme {
    pub fn theme(&self) -> theme::Theme {
        match self {
            Self::Dark => {
                let mut t = theme::system_dark();
                t.theme_type.prefer_dark(Some(true));
                t
            }
            Self::Light => {
                let mut t = theme::system_light();
                t.theme_type.prefer_dark(Some(false));
                t
            }
            Self::System => theme::system_preference(),
        }
    }
}
// reason: each bool is an independent persisted user toggle, not state that
// forms a machine; collapsing them into enums would distort the config schema.
#[allow(clippy::struct_excessive_bools)]
#[derive(CosmicConfigEntry, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[version = 2]
pub struct ViewerConfig {
    pub default_zoom: f32,
    pub fit_to_window: bool,
    pub remember_last_dir: bool,
    pub last_dir: Option<String>,
    pub smooth_scaling: bool,
    pub thumbnail_size: ThumbnailSize,
    pub cache_size: usize,
    pub show_hidden_files: bool,
    pub wallpaper_behavior: WallpaperBehavior,
    pub sort_mode: SortMode,
    pub sort_order: SortOrder,
    pub last_color: Option<[f32; 4]>,
    pub app_theme: AppTheme,
    pub show_navbar: bool,
}

impl Default for ViewerConfig {
    fn default() -> Self {
        Self {
            default_zoom: 1.0,
            fit_to_window: true,
            remember_last_dir: true,
            last_dir: None,
            smooth_scaling: true,
            thumbnail_size: ThumbnailSize::default(),
            cache_size: 20,
            show_hidden_files: false,
            wallpaper_behavior: WallpaperBehavior::default(),
            sort_mode: SortMode::default(),
            sort_order: SortOrder::default(),
            last_color: None,
            app_theme: AppTheme::System,
            show_navbar: true,
        }
    }
}

/// Open the application's `cosmic-config` handle.
///
/// # Errors
///
/// Returns a `cosmic_config::Error` if the configuration context cannot be
/// created (for example, when the config directory is inaccessible).
pub fn config() -> Result<Config, cosmic_config::Error> {
    Config::new(APP_ID, ViewerConfig::VERSION)
}
