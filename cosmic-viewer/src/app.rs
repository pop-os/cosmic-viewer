use crate::{
    fl,
    key_binds::{self, MenuAction, keyboard_shortcut_handler},
    menu::menu_bar,
    message::{ContextMessage, EditMessage, ImageMessage, NavMessage, ViewerMessage},
};
use cosmic::{
    Action, Application, ApplicationExt, Core, Element, Task,
    app::context_drawer,
    cosmic_config::CosmicConfigEntry,
    dialog::file_chooser::{self, FileFilter},
    iced::{
        Background, Length, Point, Subscription, Vector, alignment::Horizontal, clipboard,
        keyboard::on_key_press,
    },
    iced_core::Border,
    iced_widget::scrollable::{AbsoluteOffset, scroll_to},
    task::future,
    widget::{
        self, Id, button, column, container, context_menu, divider, image,
        menu::{self, KeyBind, Tree, menu_button},
        nav_bar, responsive, text, vertical_space,
    },
};
use std::{collections::HashMap, path::PathBuf};
use viewer_canvas::{CanvasImage, CanvasMessage, ClipElement, ViewerCanvas};
use viewer_config::ViewerConfig;
use viewer_core::{
    CachedImage, ClipboardImage, ImageCache, NavState, get_image_dir, image_mime_type, load_image,
    load_thumbnail, read_dpi, scan_dir,
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
    viewer_canvas: ViewerCanvas,
    context_page: Option<ContextMessage>,
    context_menu_position: Option<Point>,
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

    fn image_details_page(&self) -> Element<'_, ViewerMessage> {
        let spacing = cosmic::theme::active().cosmic().spacing;
        let mut content = column().spacing(spacing.space_m);

        let Some(path) = self.nav.current() else {
            return content.push(text::body("No image loaded")).into();
        };

        // File name
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            content = content.push(text::heading(name));
        }

        // Type
        if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
            content = content.push(detail_row(fl!("image-type"), friendly_type_name(ext)));
        }

        // File metadata (size, timestamps)
        if let Ok(meta) = std::fs::metadata(path) {
            content = content.push(detail_row(
                fl!("image-file-size"),
                format_file_size(meta.len()),
            ));

            if let Ok(created) = meta.created() {
                content = content.push(detail_row(
                    fl!("image-created"),
                    format_system_time(created),
                ));
            }

            if let Ok(modified) = meta.modified() {
                content = content.push(detail_row(
                    fl!("image-modified"),
                    format_system_time(modified),
                ));
            }

            if let Ok(accessed) = meta.accessed() {
                content = content.push(detail_row(
                    fl!("image-accessed"),
                    format_system_time(accessed),
                ));
            }
        }

        // Image dimensions
        if let Some(cached) = self.cache.get_full(path) {
            content = content.push(detail_row(
                fl!("image-size"),
                format!("{} x {}", cached.width, cached.height),
            ));
        }

        // DPI (JPEG/TIFF only)
        if let Some(dpi) = read_dpi(path) {
            content = content.push(detail_row(fl!("image-dpi"), format!("{dpi} pixels/inch")));
        }

        // Folder name
        if let Some(name) = path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
        {
            content = content.push(detail_row(fl!("image-folder"), name.to_string()));
        }

        content.into()
    }

    fn build_context_menu_element(&self) -> Element<'_, ViewerMessage> {
        let menu_item = |label: String, message: ViewerMessage| {
            menu_button(vec![text(label).into()]).on_press(message)
        };

        container(
            column()
                .push(menu_item(
                    fl!("menu-copy-to-clipboard"),
                    ViewerMessage::CopyToClipboard,
                ))
                .push(menu_item(
                    fl!("menu-copy-file-path"),
                    ViewerMessage::CopyFilePath,
                ))
                .push(menu_item(
                    fl!("menu-revert-all"),
                    ViewerMessage::Edit(EditMessage::RevertAll),
                ))
                .push(menu_item(
                    fl!("menu-image-details"),
                    ViewerMessage::Context(ContextMessage::ImageDetails),
                )),
        )
        .padding(1)
        .style(|theme| {
            let cosmic = theme.cosmic();
            let component = &cosmic.background.component;
            container::Style {
                icon_color: Some(component.on.into()),
                text_color: Some(component.on.into()),
                background: Some(Background::Color(component.base.into())),
                border: Border {
                    radius: cosmic.radius_s().map(|x| x + 1.0).into(),
                    width: 1.0,
                    color: component.divider.into(),
                },
                ..Default::default()
            }
        })
        .width(Length::Fixed(240.0))
        .into()
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
            viewer_canvas: ViewerCanvas::default(),
            context_page: None,
            context_menu_position: None,
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

    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Self::Message>> {
        let page = self.context_page?;
        let content = match page {
            ContextMessage::ImageDetails => self.image_details_page(),
            ContextMessage::About => return None,
        };

        let drawer = context_drawer::context_drawer(content, ViewerMessage::Context(page))
            .title(fl!("menu-image-details"))
            .into();
        Some(drawer)
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
        let content: Element<'_, Self::Message> = if self.viewer_canvas.image.is_some() {
            let canvas_el: Element<'_, CanvasMessage> = widget::canvas(&self.viewer_canvas)
                .width(Length::Fill)
                .height(Length::Fill)
                .into();

            let clipped: Element<'_, CanvasMessage> = ClipElement::new(canvas_el).into();
            clipped.map(ViewerMessage::Canvas)
        } else {
            column()
                .push(vertical_space().height(Length::Fill))
                .push(text("No image selected").center())
                .push(vertical_space().height(Length::Fill))
                .align_x(Horizontal::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        let mut pop = widget::popover(content);
        if let Some(point) = self.context_menu_position {
            pop = pop
                .popup(self.build_context_menu_element())
                .position(widget::popover::Position::Point(point))
                .on_close(ViewerMessage::Canvas(CanvasMessage::ContextMenu(None)));
        }

        pop.into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        let mut tasks = vec![];

        match message {
            ViewerMessage::Copy => {
                self.context_menu_position = None;
            }
            ViewerMessage::CopyToClipboard => {
                self.context_menu_position = None;
                if let Some(path) = self.nav.current().cloned()
                    && let Some(mime) = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .and_then(image_mime_type)
                {
                    if let Ok(data) = std::fs::read(&path) {
                        return clipboard::write_data(ClipboardImage {
                            data,
                            mime: mime.to_string(),
                        });
                    }
                }
            }
            ViewerMessage::CopyFilePath => {
                self.context_menu_position = None;
                if let Some(path) = self.nav.current() {
                    return cosmic::iced::clipboard::write(path.display().to_string());
                }
            }
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
                if let Some(dir) = self.nav.dir() {
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
                    self.viewer_canvas.image = None;
                    self.viewer_canvas.zoom = 1.0;
                    self.viewer_canvas.pan = Vector::ZERO;

                    self.nav.set_images(dir, images, select.as_deref());
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

                        // Reset viewport and update canvas
                        self.viewer_canvas.zoom = 1.0;
                        self.viewer_canvas.pan = Vector::ZERO;
                        if let Some(cached) = self.cache.get_full(&path) {
                            self.viewer_canvas.image = Some(CanvasImage {
                                handle: cached.handle,
                                width: cached.width,
                                height: cached.height,
                            });
                        } else {
                            self.viewer_canvas.image = None;
                        }

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
                ImageMessage::ImageReady(path) => {
                    if let Some(idx) = self.nav.index() {
                        if self.nav.current() == Some(&path) {
                            if let Some(cached) = self.cache.get_full(&path) {
                                self.viewer_canvas.image = Some(CanvasImage {
                                    handle: cached.handle,
                                    width: cached.width,
                                    height: cached.height,
                                });
                            }
                        }
                        tasks.push(self.load_nearby_thumbnails(idx, 20));
                    }
                }
                ImageMessage::LoadError(_path) => {}
            },
            ViewerMessage::Context(page) => {
                self.context_menu_position = None;
                if self.context_page == Some(page) {
                    self.context_page = None;
                } else {
                    self.context_page = Some(page);
                }

                self.set_show_context(self.context_page.is_some());
            }
            ViewerMessage::Canvas(msg) => match msg {
                CanvasMessage::ContextMenu(point) => self.context_menu_position = point,
                CanvasMessage::ZoomIn => {
                    self.viewer_canvas.zoom = (self.viewer_canvas.zoom * 1.25).min(10.0);
                }
                CanvasMessage::ZoomOut => {
                    let new_zoom = (self.viewer_canvas.zoom / 1.25).max(1.0);
                    self.viewer_canvas.zoom = new_zoom;
                    if new_zoom <= 1.0 {
                        self.viewer_canvas.pan = Vector::ZERO;
                    }
                }
                CanvasMessage::Pan(pan) => self.viewer_canvas.pan = pan,
                CanvasMessage::FitToView => {}
                CanvasMessage::Fullscreen => {}
            },
            ViewerMessage::Edit(_msg) => {
                self.context_menu_position = None;
            }
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

    fn subscription(&self) -> Subscription<Self::Message> {
        on_key_press(keyboard_shortcut_handler)
    }
}

// HELPER FUNCTIONS

fn detail_row<'a>(label: String, value: String) -> Element<'a, ViewerMessage> {
    column()
        .push(text::caption(label))
        .push(text::body(value))
        .spacing(2)
        .into()
}

fn friendly_type_name(ext: &str) -> String {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "JPEG image",
        "png" => "PNG image",
        "gif" => "GIF image",
        "webp" => "WebP image",
        "bmp" => "BMP image",
        "tiff" | "tif" => "TIFF image",
        "ico" => "ICO image",
        "avif" => "AVIF image",
        "hdr" => "HDR image",
        "jxl" => "JPEG XL image",
        "ppm" => "PPM image",
        "pgm" => "PGM image",
        "pbm" => "PBM image",
        "pnm" => "PNM image",
        "qoi" => "QOI image",
        "ff" | "farbfeld" => "Farbfeld image",
        _ => "Image",
    }
    .to_string()
}

fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;

    let human_readable = if bytes as f64 >= GB {
        format!("{:.1} GB", bytes as f64 / GB)
    } else if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else if bytes as f64 >= KB {
        format!("{:.1} KB", bytes as f64 / KB)
    } else {
        format!("{bytes} B")
    };

    format!("{human_readable} ({} bytes)", format_number(bytes))
}

fn format_number(num: u64) -> String {
    let s = num.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (idx, ch) in s.chars().enumerate() {
        if idx > 0 && (s.len() - idx) % 3 == 0 {
            result.push(',');
        }

        result.push(ch);
    }
    result
}

fn format_system_time(time: std::time::SystemTime) -> String {
    let date_time: chrono::DateTime<chrono::Local> = time.into();
    date_time.format("%a %d %b %Y %I:%M:%S %p %Z").to_string()
}
