use crate::{
    fl,
    key_binds::{self, MenuAction},
    menu::menu_bar,
    message::{ImageMessage, NavMessage, ViewerMessage},
};
use cosmic::{
    Action, Application, ApplicationExt, Core, Element, Task,
    cosmic_config::CosmicConfigEntry,
    dialog::file_chooser::{self, FileFilter},
    iced::{ContentFit, Length, alignment::Horizontal},
    iced_widget::scrollable::{AbsoluteOffset, scroll_to},
    task::future,
    widget::{
        Id, column, container, image, menu::KeyBind, nav_bar, responsive, text, vertical_space,
    },
};
use std::{collections::HashMap, path::PathBuf};
use viewer_config::ViewerConfig;
use viewer_core::{
    CachedImage, ImageCache, NavState, get_image_dir, load_image, load_thumbnail, scan_dir,
};
use viewer_widgets::{GridItem, ImageGrid, image_grid::image_grid};

pub struct CosmicViewer {
    core: Core,
    key_binds: HashMap<KeyBind, MenuAction>,
    config: ViewerConfig,
    nav: NavState,
    nav_bar_model: nav_bar::Model,
    cache: ImageCache,
    grid: ImageGrid<'static, Action<ViewerMessage>>,
    scroll_id: Id,
}

impl CosmicViewer {
    fn open_path(&mut self, path: PathBuf) -> Task<Action<ViewerMessage>> {
        let Some(dir) = get_image_dir(&path) else {
            return Task::none();
        };

        let select = path.is_file().then_some(path);
        let include_hidden = self.config.show_hidden_files;
        let sort_mode = self.config.sort_mode;
        let sort_order = self.config.sort_order;
        let dir_clone = dir.clone();

        future(async move {
            let images = scan_dir(&dir, include_hidden, sort_mode, sort_order).await;
            Action::App(ViewerMessage::Nav(NavMessage::ScanComplete(
                dir_clone, images, select,
            )))
        })
    }

    fn rebuild_grid_items(&mut self) {
        let items = self
            .nav
            .images()
            .iter()
            .map(|path| {
                let handle = self.cache.get_thumbnail(path);
                GridItem::new(path.clone(), handle, 0, 0)
            })
            .collect();

        self.grid.set_items(items);
        self.grid.set_focused(self.nav.index());
        self.grid
            .set_selected(self.nav.index().into_iter().collect());
    }

    fn load_nearby_thumbnails(&self, center: usize, radius: usize) -> Task<Action<ViewerMessage>> {
        let max_size = self.config.thumbnail_size.pixels();
        let images = self.nav.images();
        let total = images.len();

        let start = center.saturating_sub(radius);
        let end = (center + radius + 1).min(total);

        let tasks: Vec<_> = images[start..end]
            .iter()
            .filter(|path| {
                self.cache.get_thumbnail(path).is_none() && !self.cache.is_thumbnail_pending(path)
            })
            .cloned()
            .map(|path| {
                self.cache.set_thumbnail_pending(path.clone());
                let cache = self.cache.clone();
                future(async move {
                    match load_thumbnail(path.clone(), max_size).await {
                        Ok(loaded) => {
                            cache.insert_thumbnail(loaded.path.clone(), loaded.handle);
                            Action::App(ViewerMessage::Image(ImageMessage::ThumbnailReady(
                                loaded.path,
                                loaded.width,
                                loaded.height,
                            )))
                        }
                        Err(_) => Action::App(ViewerMessage::Image(ImageMessage::LoadError(path))),
                    }
                })
            })
            .collect();

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn load_remaining_thumbnails(&self) -> Task<Action<ViewerMessage>> {
        let max_size = self.config.thumbnail_size.pixels();

        let tasks: Vec<_> = self
            .nav
            .images()
            .iter()
            .filter(|path| {
                self.cache.get_thumbnail(path).is_none() && !self.cache.is_thumbnail_pending(path)
            })
            .cloned()
            .map(|path| {
                self.cache.set_thumbnail_pending(path.clone());
                let cache = self.cache.clone();
                future(async move {
                    match load_thumbnail(path.clone(), max_size).await {
                        Ok(loaded) => {
                            cache.insert_thumbnail(loaded.path.clone(), loaded.handle);
                            Action::App(ViewerMessage::Image(ImageMessage::ThumbnailReady(
                                loaded.path,
                                loaded.width,
                                loaded.height,
                            )))
                        }
                        Err(_) => Action::App(ViewerMessage::Image(ImageMessage::LoadError(path))),
                    }
                })
            })
            .collect();

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn load_full_image(&mut self, path: PathBuf) -> Task<Action<ViewerMessage>> {
        if self.cache.get_full(&path).is_some() || self.cache.is_pending(&path) {
            return Task::none();
        }

        self.cache.set_pending(path.clone());
        let cache = self.cache.clone();
        future(async move {
            match load_image(path.clone()).await {
                Ok(loaded) => {
                    cache.insert_full(
                        loaded.path.clone(),
                        CachedImage {
                            handle: loaded.handle,
                            width: loaded.width,
                            height: loaded.height,
                        },
                    );
                    Action::App(ViewerMessage::Image(ImageMessage::ImageReady(loaded.path)))
                }
                Err(_) => Action::App(ViewerMessage::Image(ImageMessage::LoadError(path))),
            }
        })
    }
}

impl Application for CosmicViewer {
    const APP_ID: &'static str = "com.system76.CosmicViewer";

    type Executor = cosmic::executor::Default;
    type Flags = Option<PathBuf>;
    type Message = ViewerMessage;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let mut tasks = vec![];

        let config = viewer_config::config()
            .ok()
            .as_ref()
            .and_then(|h| ViewerConfig::get_entry(h).ok())
            .unwrap_or_default();

        let scroll_id = Id::unique();

        let grid = ImageGrid::new(Vec::new())
            .thumbnail_size(config.thumbnail_size.pixels())
            .on_activate(|idx| Action::App(ViewerMessage::Nav(NavMessage::GridActivate(idx))))
            .on_focus(|idx| Action::App(ViewerMessage::Nav(NavMessage::GridFocus(idx))))
            .on_scroll_request(|req| {
                Action::App(ViewerMessage::Nav(NavMessage::GridScroll(req.offset_y)))
            })
            .scrollable(scroll_id.clone());

        let mut viewer = Self {
            core,
            key_binds: key_binds::init_keybinds(),
            config,
            nav: NavState::new(),
            nav_bar_model: nav_bar::Model::default(),
            cache: ImageCache::with_defaults(),
            grid,
            scroll_id,
        };

        tasks
            .push(viewer.set_window_title(fl!("app-title"), viewer.core.main_window_id().unwrap()));

        if let Some(path) = flags {
            tasks.push(viewer.open_path(path));
        }

        (viewer, Task::batch(tasks))
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![menu_bar(&self.core, &self.key_binds, &vec![])]
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_bar_model)
    }

    fn nav_bar(&self) -> Option<Element<'_, Action<Self::Message>>> {
        if !self.core().nav_bar_active() {
            return None;
        }

        let thumbnail_size = self.config.thumbnail_size.pixels();
        let col_spacing: f32 = 8.0;
        let panel_width = thumbnail_size as f32 + col_spacing * 2.0 + 20.;

        let grid = image_grid(self.grid.items().to_vec())
            .thumbnail_size(thumbnail_size)
            .focused(self.grid.get_focused())
            .selected(self.grid.get_selected().to_vec())
            .on_activate(|idx| Action::App(ViewerMessage::Nav(NavMessage::GridActivate(idx))))
            .on_focus(|idx| Action::App(ViewerMessage::Nav(NavMessage::GridFocus(idx))))
            .on_scroll_request(|req| {
                Action::App(ViewerMessage::Nav(NavMessage::GridScroll(req.offset_y)))
            })
            .scrollable(self.scroll_id.clone())
            .into_element();

        Some(
            container(grid)
                .width(Length::Fixed(panel_width))
                .height(Length::Fill)
                .class(cosmic::theme::Container::custom(nav_bar::nav_bar_style))
                .into(),
        )
    }

    fn view(&self) -> Element<'_, Self::Message> {
        if let Some(path) = self.nav.current()
            && let Some(cached) = self.cache.get_full(path)
        {
            let handle = cached.handle.clone();
            let img_width = cached.width as f32;
            let img_height = cached.height as f32;

            responsive(move |size| {
                let spacing = cosmic::theme::active().cosmic().spacing;
                let pad = (spacing.space_xs * 2) as f32;
                let avail_width = size.width - pad;
                let avail_height = size.height - pad;

                let scale = (avail_width / img_width).min(avail_height / img_height);
                let scaled_width = img_width * scale;
                let scaled_height = img_height * scale;

                container(
                    image(handle.clone())
                        .content_fit(ContentFit::Fill)
                        .width(Length::Fixed(scaled_width))
                        .height(Length::Fixed(scaled_height)),
                )
                .center(Length::Fill)
                .into()
            })
            .into()
        } else {
            column()
                .push(vertical_space().height(Length::Fill))
                .push(text("No image selected").center())
                .push(vertical_space().height(Length::Fill))
                .align_x(Horizontal::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        let mut tasks = vec![];

        match message {
            ViewerMessage::Copy => {}
            ViewerMessage::CopyToClipboard => {}
            ViewerMessage::Cut => {}
            ViewerMessage::OpenFileDialog => {
                return future(async move {
                    let dialog = file_chooser::open::Dialog::new()
                        .title("Open Image")
                        .filter(
                            FileFilter::new("All")
                                .extension("jpg")
                                .extension("jpeg")
                                .extension("png")
                                .extension("gif")
                                .extension("webp")
                                .extension("bmp")
                                .extension("tiff")
                                .extension("tif")
                                .extension("ico")
                                .extension("avif")
                                .extension("hdr"),
                        )
                        .filter(FileFilter::new("jpeg").extension("jpg").extension("jpeg"))
                        .filter(FileFilter::new("png").extension("png"))
                        .filter(FileFilter::new("gif").extension("gif"))
                        .filter(FileFilter::new("webp").extension("webp"))
                        .filter(FileFilter::new("bmp").extension("bmp"))
                        .filter(FileFilter::new("tiff").extension("tiff").extension("tif"))
                        .filter(FileFilter::new("avif").extension("avif"))
                        .filter(FileFilter::new("ico").extension("ico"))
                        .filter(FileFilter::new("hdr").extension("hdr"));

                    match dialog.open_file().await {
                        Ok(response) => {
                            if let Ok(path) = response.url().to_file_path() {
                                Action::App(ViewerMessage::Open(path))
                            } else {
                                Action::App(ViewerMessage::Cancelled)
                            }
                        }
                        Err(file_chooser::Error::Cancelled) => {
                            Action::App(ViewerMessage::Cancelled)
                        }
                        Err(e) => {
                            tracing::error!("File dialog error: {e}");
                            Action::App(ViewerMessage::Cancelled)
                        }
                    }
                });
            }
            ViewerMessage::OpenFolderDialog => {
                return future(async move {
                    let dialog = file_chooser::open::Dialog::new().title("Open Folder");

                    match dialog.open_folder().await {
                        Ok(response) => {
                            if let Ok(path) = response.url().to_file_path() {
                                Action::App(ViewerMessage::Open(path))
                            } else {
                                Action::App(ViewerMessage::Cancelled)
                            }
                        }
                        Err(file_chooser::Error::Cancelled) => {
                            Action::App(ViewerMessage::Cancelled)
                        }
                        Err(e) => {
                            tracing::error!("Folder dialog error: {e}");
                            Action::App(ViewerMessage::Cancelled)
                        }
                    }
                });
            }
            ViewerMessage::Open(path) => tasks.push(self.open_path(path)),
            ViewerMessage::OpenRecent(_idx) => {}
            ViewerMessage::OpenContaining => {
                if let Some(path) = self.nav.current()
                    && let Some(dir) = path.parent()
                {
                    if let Err(e) = open::that(dir) {
                        eprintln!("Failed to open containing folder: {e}");
                    }
                }
            }
            ViewerMessage::Paste => {}
            ViewerMessage::CloseFile => {}
            ViewerMessage::Save => {}
            ViewerMessage::SaveAs => {}
            ViewerMessage::Share => {}
            ViewerMessage::Print => {}
            ViewerMessage::Cancelled => {}
            ViewerMessage::Quit => {}
            ViewerMessage::Nav(msg) => match msg {
                NavMessage::ScanComplete(dir, images, select) => {
                    let dir = dir.clone();
                    self.nav.set_images(dir.clone(), images, select.as_deref());
                    self.rebuild_grid_items();

                    // If no image is selected on load, start loading from image 1 (idx = 0)
                    if !self.nav.is_empty() {
                        tasks.push(self.load_remaining_thumbnails());
                    }

                    // Load the selected image and the 40 images around it (20 up and 20 down)
                    if let Some(idx) = self.nav.index() {
                        if let Some(path) = self.nav.current().cloned() {
                            tasks.push(self.load_full_image(path));
                        }

                        let thumb_size = self.config.thumbnail_size.pixels() as f32;
                        let button_padding = 8.0;
                        let cell_size = thumb_size + (button_padding * 2.0);
                        let row_spacing = 8.0;
                        let offset = idx as f32 * (cell_size + row_spacing);
                        tasks.push(scroll_to(
                            self.scroll_id.clone(),
                            AbsoluteOffset { x: 0.0, y: offset },
                        ));
                    }
                }
                NavMessage::GridActivate(idx) => {
                    if let Some(path) = self.nav.select(idx) {
                        let path = path.clone();
                        self.grid.set_focused(Some(idx));
                        tasks.push(self.load_full_image(path));
                    }
                }
                NavMessage::GridFocus(idx) => self.grid.set_focused(Some(idx)),
                NavMessage::GridScroll(offset) => {
                    tasks.push(scroll_to(
                        self.scroll_id.clone(),
                        AbsoluteOffset { x: 0.0, y: offset },
                    ));
                }
            },
            ViewerMessage::Image(msg) => match msg {
                ImageMessage::ThumbnailReady(path, width, height) => {
                    if let Some(handle) = self.cache.get_thumbnail(&path)
                        && let Some(item) = self
                            .grid
                            .items_mut()
                            .iter_mut()
                            .find(|item| item.path == path)
                    {
                        item.handle = Some(handle);
                        item.width = width;
                        item.height = height;
                    }

                    tasks.push(self.load_remaining_thumbnails());
                }
                ImageMessage::ImageReady(_path) => {
                    if let Some(idx) = self.nav.index() {
                        tasks.push(self.load_nearby_thumbnails(idx, 20));
                    }
                }
                ImageMessage::LoadError(_path) => {}
            },
            ViewerMessage::Context(_msg) => {}
            ViewerMessage::Viewport(_msg) => {}
            ViewerMessage::Edit(_msg) => {}
            ViewerMessage::Surface(action) => {
                return cosmic::task::message(Action::Cosmic(cosmic::app::Action::Surface(action)));
            }
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }
}
