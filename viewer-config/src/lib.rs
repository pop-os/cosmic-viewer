use cosmic::cosmic_config::{self, Config, ConfigGet, ConfigSet, CosmicConfigEntry};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const CONFIG_VERSION: u64 = 1;
const APP_ID: &str = "com.system76.CosmicViewer";

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
            WallpaperBehavior::Ask => write!(f, "Always Ask"),
            WallpaperBehavior::AllDisplays => write!(f, "All Displays"),
            WallpaperBehavior::CurrentDisplay => write!(f, "Current Display"),
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
            SortMode::Name => write!(f, "Name"),
            SortMode::Date => write!(f, "Date"),
            SortMode::Size => write!(f, "Size"),
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

    pub fn toggle(self) -> Self {
        match self {
            SortOrder::Ascending => SortOrder::Descending,
            SortOrder::Descending => SortOrder::Ascending,
        }
    }
}

impl fmt::Display for SortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SortOrder::Ascending => write!(f, "Ascending"),
            SortOrder::Descending => write!(f, "Descending"),
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
    pub fn pixels(self) -> u32 {
        match self {
            ThumbnailSize::Small => 64,
            ThumbnailSize::Medium => 128,
            ThumbnailSize::Large => 192,
            ThumbnailSize::XLarge => 256,
        }
    }
}

/// Maximum number of recent folders to remember
pub const MAX_RECENT_FOLDERS: usize = 10;
/// Maximum number of recent colors to remember
pub const MAX_RECENT_COLORS: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub recent_folders: Vec<String>,
    pub last_color: Option<[f32; 4]>,
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
            recent_folders: Vec::new(),
            last_color: None,
        }
    }
}

impl ViewerConfig {
    /// Add a folder to the recent folders list.
    /// The most recent folder is at index 0.
    /// Duplicates are moved to the front.
    pub fn add_recent_folder(&mut self, folder: String) {
        // Remove if already exists (we'll add it to front)
        self.recent_folders.retain(|f| f != &folder);
        // Add to front
        self.recent_folders.insert(0, folder);
        // Trim to max size
        self.recent_folders.truncate(MAX_RECENT_FOLDERS);
    }
}

impl CosmicConfigEntry for ViewerConfig {
    const VERSION: u64 = CONFIG_VERSION;

    fn write_entry(&self, config: &cosmic_config::Config) -> Result<(), cosmic_config::Error> {
        config.set("default_zoom", self.default_zoom)?;
        config.set("fit_to_window", self.fit_to_window)?;
        config.set("remember_last_dir", self.remember_last_dir)?;
        config.set("last_dir", self.last_dir.clone())?;
        config.set("smooth_scaling", self.smooth_scaling)?;
        config.set("thumbnail_size", self.thumbnail_size)?;
        config.set("cache_size", self.cache_size)?;
        config.set("show_hidden_files", self.show_hidden_files)?;
        config.set("wallpaper_behavior", self.wallpaper_behavior)?;
        config.set("sort_mode", self.sort_mode)?;
        config.set("sort_order", self.sort_order)?;
        config.set("recent_folders", self.recent_folders.clone())?;
        config.set("last_color", self.last_color)?;
        Ok(())
    }

    fn get_entry(
        config: &cosmic_config::Config,
    ) -> Result<Self, (Vec<cosmic_config::Error>, Self)> {
        let mut errors = Vec::new();
        let mut cfg = ViewerConfig::default();

        macro_rules! get_field {
            ($name:literal, $field:ident, $type:ty) => {
                match config.get::<$type>($name) {
                    Ok(val) => cfg.$field = val,
                    Err(e) => errors.push(e),
                }
            };
        }

        get_field!("default_zoom", default_zoom, f32);
        get_field!("fit_to_window", fit_to_window, bool);
        get_field!("remember_last_dir", remember_last_dir, bool);
        get_field!("last_dir", last_dir, Option<String>);
        get_field!("smooth_scaling", smooth_scaling, bool);
        get_field!("thumbnail_size", thumbnail_size, ThumbnailSize);
        get_field!("cache_size", cache_size, usize);
        get_field!("show_hidden_files", show_hidden_files, bool);
        get_field!("wallpaper_behavior", wallpaper_behavior, WallpaperBehavior);
        get_field!("sort_mode", sort_mode, SortMode);
        get_field!("sort_order", sort_order, SortOrder);
        get_field!("recent_folders", recent_folders, Vec<String>);
        get_field!("last_color", last_color, Option<[f32; 4]>);

        if errors.is_empty() {
            Ok(cfg)
        } else {
            Err((errors, cfg))
        }
    }

    fn update_keys<T: AsRef<str>>(
        &mut self,
        config: &cosmic_config::Config,
        changed_keys: &[T],
    ) -> (Vec<cosmic_config::Error>, Vec<&'static str>) {
        let mut errors = Vec::new();
        let mut updated = Vec::new();

        for key in changed_keys {
            match key.as_ref() {
                "default_zoom" => match config.get::<f32>("default_zoom") {
                    Ok(val) => {
                        self.default_zoom = val;
                        updated.push("default_zoom");
                    }
                    Err(e) => errors.push(e),
                },
                "fit_to_window" => match config.get::<bool>("fit_to_window") {
                    Ok(val) => {
                        self.fit_to_window = val;
                        updated.push("fit_to_window");
                    }
                    Err(e) => errors.push(e),
                },
                "show_hidden_files" => match config.get::<bool>("show_hidden_files") {
                    Ok(val) => {
                        self.show_hidden_files = val;
                        updated.push("show_hidden_files");
                    }
                    Err(e) => errors.push(e),
                },
                "sort_mode" => match config.get::<SortMode>("sort_mode") {
                    Ok(val) => {
                        self.sort_mode = val;
                        updated.push("sort_mode");
                    }
                    Err(e) => errors.push(e),
                },
                "sort_order" => match config.get::<SortOrder>("sort_order") {
                    Ok(val) => {
                        self.sort_order = val;
                        updated.push("sort_order");
                    }
                    Err(e) => errors.push(e),
                },
                _ => {}
            }
        }

        (errors, updated)
    }
}

pub fn config() -> Result<Config, cosmic_config::Error> {
    Config::new(APP_ID, CONFIG_VERSION)
}
