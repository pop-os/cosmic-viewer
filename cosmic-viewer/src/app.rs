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
        self, Background, Color, Length, Point, Subscription, Vector,
        alignment::Horizontal,
        clipboard, event,
        keyboard::{
            key::{Key, Named},
            on_key_press,
        },
    },
    iced_core::Border,
    iced_widget::scrollable::{AbsoluteOffset, scroll_to},
    task::future,
    widget::{
        self, Id, Space, button, column, container, dropdown, icon,
        menu::{KeyBind, menu_button},
        nav_bar, text, vertical_space,
    },
};
use image::DynamicImage;
use std::{collections::HashMap, path::PathBuf};
use viewer_canvas::{CanvasImage, CanvasMessage, ToolKind, ViewportManager};
use viewer_config::ViewerConfig;
use viewer_core::{
    CachedImage, ClipboardImage, ImageCache, NavState, get_image_dir, image_mime_type, load_image,
    load_thumbnail, read_dpi, scan_dir,
};
use viewer_toolbar::{ItemPriority, ToolbarItem, ToolbarMode, responsive_toolbar};
use viewer_tools::{
    ToolOperation,
    annotate::{
        AnnotateColor, AnnotateTool, HighlighterPreview, PenPreview, PencilPreview, ShapeKind,
        ShapePreview, TextOperation, TextPreview,
    },
    crop::{CropRatio, CropSelection},
    rotate::{RotateDirection, RotateOperation},
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
    viewport: ViewportManager,
    context_page: Option<ContextMessage>,
    context_menu_position: Option<Point>,
    annotate_tool: AnnotateTool,
    annotate_color: AnnotateColor,
    annotate_stroke_size: f32,
    crop_ratio: CropRatio,
    text_editing: bool,
    toolbar_overflow_open: bool,
    window_width: Option<f32>,
    is_fullscreen: bool,
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

    /// Rebuild the viewport image from the correct base (working_image
    /// or cache original).
    /// Used after dimension-changing operations like crop and rotate.
    fn rebuild_viewport(&mut self) {
        let base = self.viewport.working_image().cloned().or_else(|| {
            self.nav
                .current()
                .and_then(|path| self.cache.get_full(path))
                .map(|cached| cached.image.clone())
        });

        if let Some(base) = base {
            self.viewport.rebuild_image(&base);
        }
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
                            image: loaded.image,
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

    fn build_toolbar(&self) -> Element<'_, ViewerMessage> {
        match self.viewport.active_tool() {
            Some(ToolKind::Crop) => self.build_crop_toolbar(),
            Some(ToolKind::Annotate) => self.build_annotate_toolbar(),
            _ => self.build_default_toolbar(),
        }
    }

    fn build_default_toolbar(&self) -> Element<'_, ViewerMessage> {
        let mode = ToolbarMode::from_width(self.window_width.expect("Window has width"));

        let icon_btn = |name: &'static str,
                        tooltip: String,
                        msg: ViewerMessage|
         -> Element<'_, ViewerMessage> {
            button::icon(icon::from_name(name))
                .tooltip(tooltip)
                .on_press(msg)
                .into()
        };

        let zoom_pct = format!("{}%", (self.viewport.zoom() * 100.0).round() as u32);

        responsive_toolbar(mode)
            .overflow_open(self.toolbar_overflow_open)
            .start(
                ToolbarItem::new(icon_btn(
                    "edit-symbolic",
                    fl!("toolbar-annotate"),
                    ViewerMessage::Edit(EditMessage::Annotate),
                ))
                .priority(ItemPriority::Standard)
                .overflow(
                    fl!("toolbar-annotate"),
                    Some("edit-symbolic"),
                    ViewerMessage::Edit(EditMessage::Annotate),
                ),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "edit-cut-symbolic",
                    fl!("toolbar-crop"),
                    ViewerMessage::Edit(EditMessage::Crop),
                ))
                .priority(ItemPriority::Standard)
                .overflow(
                    fl!("toolbar-crop"),
                    Some("edit-cut-symbolic"),
                    ViewerMessage::Edit(EditMessage::Crop),
                ),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "object-rotate-left-symbolic",
                    fl!("toolbar-rotate-left"),
                    ViewerMessage::Edit(EditMessage::RotateLeft),
                ))
                .priority(ItemPriority::Optional)
                .overflow(
                    fl!("toolbar-rotate-left"),
                    Some("object-rotate-left-symbolic"),
                    ViewerMessage::Edit(EditMessage::RotateLeft),
                ),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "object-rotate-right-symbolic",
                    fl!("toolbar-rotate-right"),
                    ViewerMessage::Edit(EditMessage::RotateRight),
                ))
                .priority(ItemPriority::Optional)
                .overflow(
                    fl!("toolbar-rotate-right"),
                    Some("object-rotate-right-symbolic"),
                    ViewerMessage::Edit(EditMessage::RotateRight),
                ),
            )
            .center(
                ToolbarItem::new(icon_btn(
                    "list-remove-symbolic",
                    fl!("toolbar-zoom-out"),
                    ViewerMessage::Canvas(CanvasMessage::ZoomOut),
                ))
                .priority(ItemPriority::Essential),
            )
            .center(ToolbarItem::new(text::body(zoom_pct)).priority(ItemPriority::Essential))
            .center(
                ToolbarItem::new(icon_btn(
                    "list-add-symbolic",
                    fl!("toolbar-zoom-in"),
                    ViewerMessage::Canvas(CanvasMessage::ZoomIn),
                ))
                .priority(ItemPriority::Essential),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "document-save-symbolic",
                    fl!("toolbar-save"),
                    ViewerMessage::Save,
                ))
                .priority(ItemPriority::Optional)
                .overflow(
                    fl!("toolbar-save"),
                    Some("document-save-symbolic"),
                    ViewerMessage::Save,
                ),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "document-save-as-symbolic",
                    fl!("toolbar-save-as"),
                    ViewerMessage::SaveAs,
                ))
                .priority(ItemPriority::Optional)
                .overflow(
                    fl!("toolbar-save-as"),
                    Some("document-save-as-symbolic"),
                    ViewerMessage::SaveAs,
                ),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "view-fullscreen-symbolic",
                    fl!("toolbar-fullscreen"),
                    ViewerMessage::Canvas(CanvasMessage::Fullscreen),
                ))
                .priority(ItemPriority::Essential),
            )
            .view(|| ViewerMessage::ToolbarOverflowToggle)
    }

    fn build_crop_toolbar(&self) -> Element<'_, ViewerMessage> {
        let mode = ToolbarMode::from_width(self.window_width.expect("Window has width"));

        let icon_btn = |name: &'static str,
                        tooltip: String,
                        msg: ViewerMessage|
         -> Element<'_, ViewerMessage> {
            button::icon(icon::from_name(name))
                .tooltip(tooltip)
                .on_press(msg)
                .into()
        };

        let is_portrait = self
            .viewport
            .image_size()
            .map(|size| size.height > size.width)
            .unwrap_or(false);
        let presets = CropRatio::presets();
        let labels: Vec<String> = presets
            .iter()
            .map(|ratio| ratio.label(is_portrait).to_string())
            .collect();
        let selected = presets.iter().position(|ratio| *ratio == self.crop_ratio);

        responsive_toolbar(mode)
            .overflow_open(self.toolbar_overflow_open)
            .start(
                ToolbarItem::new(icon_btn(
                    "edit-undo-symbolic",
                    fl!("menu-undo"),
                    ViewerMessage::Edit(EditMessage::Undo),
                ))
                .priority(ItemPriority::Essential),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "edit-redo-symbolic",
                    fl!("menu-redo"),
                    ViewerMessage::Edit(EditMessage::Redo),
                ))
                .priority(ItemPriority::Essential),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "object-rotate-left-symbolic",
                    fl!("menu-rotate-left"),
                    ViewerMessage::Edit(EditMessage::RotateLeft),
                ))
                .priority(ItemPriority::Optional)
                .overflow(
                    fl!("menu-rotate-left"),
                    Some("object-rotate-left-symbolic"),
                    ViewerMessage::Edit(EditMessage::RotateLeft),
                ),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "object-rotate-right-symbolic",
                    fl!("menu-rotate-right"),
                    ViewerMessage::Edit(EditMessage::RotateRight),
                ))
                .priority(ItemPriority::Optional)
                .overflow(
                    fl!("menu-rotate-right"),
                    Some("object-rotate-right-symbolic"),
                    ViewerMessage::Edit(EditMessage::RotateRight),
                ),
            )
            .center(
                ToolbarItem::new(dropdown(labels, selected, |idx| {
                    ViewerMessage::Edit(EditMessage::CropRatio(CropRatio::presets()[idx]))
                }))
                .priority(ItemPriority::Essential),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "window-close-symbolic",
                    fl!("toolbar-cancel"),
                    ViewerMessage::Edit(EditMessage::CropCancel),
                ))
                .priority(ItemPriority::Essential),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "object-select-symbolic",
                    fl!("toolbar-apply"),
                    ViewerMessage::Edit(EditMessage::CropApply),
                ))
                .priority(ItemPriority::Essential),
            )
            .view(|| ViewerMessage::ToolbarOverflowToggle)
    }

    fn build_annotate_toolbar(&self) -> Element<'_, ViewerMessage> {
        let mode =
            ToolbarMode::from_width(self.window_width.expect("Window size should not be none"));

        let icon_btn = |name: &'static str,
                        tooltip: String,
                        msg: ViewerMessage|
         -> Element<'_, ViewerMessage> {
            button::icon(icon::from_name(name))
                .tooltip(tooltip)
                .on_press(msg)
                .into()
        };

        let mut toolbar = responsive_toolbar(mode)
            .start(
                ToolbarItem::new(icon_btn(
                    "edit-undo-symbolic",
                    fl!("menu-undo"),
                    ViewerMessage::Edit(EditMessage::Undo),
                ))
                .priority(ItemPriority::Optional)
                .overflow(
                    fl!("menu-undo"),
                    Some("edit-undo-symbolic"),
                    ViewerMessage::Edit(EditMessage::Undo),
                ),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "edit-redo-symbolic",
                    fl!("menu-redo"),
                    ViewerMessage::Edit(EditMessage::Redo),
                ))
                .priority(ItemPriority::Optional)
                .overflow(
                    fl!("menu-redo"),
                    Some("edit-redo-symbolic"),
                    ViewerMessage::Edit(EditMessage::Redo),
                ),
            )
            .center(
                ToolbarItem::new(icon_btn(
                    AnnotateTool::Text.icon_name(),
                    fl!("text-tool"),
                    ViewerMessage::Edit(EditMessage::AnnotateTool(AnnotateTool::Text)),
                ))
                .priority(ItemPriority::Essential),
            )
            .center(
                ToolbarItem::new(dropdown(
                    vec![fl!("drawing-pen"), fl!("drawing-pencil")],
                    AnnotateTool::draw_tools()
                        .iter()
                        .position(|tool| *tool == self.annotate_tool),
                    |idx| {
                        ViewerMessage::Edit(EditMessage::AnnotateTool(
                            AnnotateTool::draw_tools()[idx],
                        ))
                    },
                ))
                .priority(ItemPriority::Essential),
            )
            .center(
                ToolbarItem::new(icon_btn(
                    AnnotateTool::Highlighter.icon_name(),
                    fl!("drawing-highlighter"),
                    ViewerMessage::Edit(EditMessage::AnnotateTool(AnnotateTool::Highlighter)),
                ))
                .priority(ItemPriority::Essential),
            )
            .center(
                ToolbarItem::new(dropdown(
                    vec![
                        fl!("shapes-rectangle"),
                        fl!("shapes-ellipse"),
                        fl!("shapes-arrow"),
                        fl!("shapes-line"),
                        fl!("shapes-star"),
                        fl!("shapes-polygon"),
                    ],
                    AnnotateTool::shape_tools()
                        .iter()
                        .position(|tool| *tool == self.annotate_tool),
                    |idx| {
                        ViewerMessage::Edit(EditMessage::AnnotateTool(
                            AnnotateTool::shape_tools()[idx],
                        ))
                    },
                ))
                .priority(ItemPriority::Essential),
            );

        let colors = AnnotateColor::presets();
        for color in &colors {
            let c = *color;
            let is_selected = c == self.annotate_color;
            toolbar = toolbar.center(
                ToolbarItem::new(
                    button::custom(container(Space::new(12, 12)).class(
                        cosmic::theme::Container::custom(move |_theme| container::Style {
                            background: Some(Background::Color(c.0)),
                            border: Border {
                                radius: 6.0.into(),
                                width: if is_selected { 2.0 } else { 1.0 },
                                color: if is_selected {
                                    Color::WHITE
                                } else {
                                    Color::from_rgba(1.0, 1.0, 1.0, 0.3)
                                },
                            },
                            ..Default::default()
                        }),
                    ))
                    .on_press(ViewerMessage::Edit(EditMessage::AnnotateColor(c))),
                )
                .priority(ItemPriority::Standard),
            );
        }

        let sizes = [2., 4., 6., 8., 10.];
        toolbar = toolbar.center(
            ToolbarItem::new(dropdown(
                vec!["2px", "4px", "6px", "8px", "10px"],
                sizes
                    .iter()
                    .position(|size| *size == self.annotate_stroke_size),
                |idx| ViewerMessage::Edit(EditMessage::AnnotateStroke(idx)),
            ))
            .priority(ItemPriority::Standard),
        );

        toolbar = toolbar
            .end(
                ToolbarItem::new(icon_btn(
                    "window-close-symbolic",
                    fl!("toolbar-cancel"),
                    ViewerMessage::Edit(EditMessage::AnnotateCancel),
                ))
                .priority(ItemPriority::Essential),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "object-select-symbolic",
                    fl!("toolbar-apply"),
                    ViewerMessage::Edit(EditMessage::AnnotateApply),
                ))
                .priority(ItemPriority::Essential),
            );

        toolbar.view(|| ViewerMessage::ToolbarOverflowToggle)
    }

    fn flatten_image(&self) -> Option<DynamicImage> {
        let base = if let Some(working) = self.viewport.working_image() {
            working.clone()
        } else {
            let current = self.nav.current()?;
            let cached = self.cache.get_full(current)?;
            cached.image.clone()
        };

        let mut image = base;
        for op in self.viewport.operations() {
            op.apply(&mut image);
        }

        Some(image)
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
            viewport: ViewportManager::default(),
            context_page: None,
            context_menu_position: None,
            annotate_tool: AnnotateTool::default(),
            annotate_color: AnnotateColor::default(),
            annotate_stroke_size: 2.,
            crop_ratio: CropRatio::Custom,
            text_editing: false,
            toolbar_overflow_open: false,
            window_width: Some(0.0),
            is_fullscreen: false,
        };

        tasks
            .push(viewer.set_window_title(fl!("app-title"), viewer.core.main_window_id().unwrap()));

        if let Some(path) = flags {
            tasks.push(viewer.open_path(path));
        }

        (viewer, Task::batch(tasks))
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![menu_bar(&self.core, &self.key_binds, &[])]
    }

    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Self::Message>> {
        let page = self.context_page?;
        let content = match page {
            ContextMessage::ImageDetails => self.image_details_page(),
            ContextMessage::About => return None,
        };

        let drawer = context_drawer::context_drawer(content, ViewerMessage::Context(page))
            .title(fl!("menu-image-details"));
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
        let content: Element<'_, Self::Message> = if self.viewport.image().is_some() {
            self.viewport.element().map(ViewerMessage::Canvas)
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

        let spacing = cosmic::theme::active().cosmic().spacing;

        let main = column()
            .push(content)
            .push(
                container(self.build_toolbar())
                    .center_x(Length::Fill)
                    .padding([spacing.space_xxs, 0, spacing.space_xxs, 0]),
            )
            .width(Length::Fill)
            .height(Length::Fill);

        let mut pop = widget::popover(main);
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
                    && let Ok(data) = std::fs::read(&path)
                {
                    return clipboard::write_data(ClipboardImage {
                        data,
                        mime: mime.to_string(),
                    });
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
                if let Some(dir) = self.nav.dir()
                    && let Err(e) = open::that(dir)
                {
                    eprintln!("Failed to open containing folder: {e}");
                }
            }
            ViewerMessage::Paste => {}
            ViewerMessage::CloseFile => {}
            ViewerMessage::Save => {
                if let (Some(image), Some(path)) = (self.flatten_image(), self.nav.current()) {
                    if let Err(e) = image.save(path) {
                        tracing::error!("Failed to save image: {e}");
                    } else {
                        self.viewport.revert_all();
                        self.cache.remove_full(path);
                        self.viewport.cancel_tool();
                        tasks.push(self.load_full_image(path.clone()));
                    }
                }
            }
            ViewerMessage::SaveAs => {
                let suggested = self
                    .nav
                    .current()
                    .and_then(|path| {
                        path.file_name()
                            .map(|name| name.to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| "image.png".to_string());

                return future(async move {
                    let dialog = file_chooser::save::Dialog::new()
                        .title("Save Image As...".to_string())
                        .file_name(suggested);

                    match dialog.save_file().await {
                        Ok(response) => {
                            if let Ok(path) = response
                                .url()
                                .expect("response.url should be vaild")
                                .to_file_path()
                            {
                                Action::App(ViewerMessage::SavedAs(path))
                            } else {
                                Action::App(ViewerMessage::Cancelled)
                            }
                        }
                        Err(file_chooser::Error::Cancelled) => {
                            Action::App(ViewerMessage::Cancelled)
                        }
                        Err(e) => {
                            tracing::error!("Save dialog error: {e}");
                            Action::App(ViewerMessage::Cancelled)
                        }
                    }
                });
            }
            ViewerMessage::SavedAs(path) => {
                if let Some(image) = self.flatten_image()
                    && let Err(e) = image.save(&path)
                {
                    tracing::error!("Failed to save image: {e}");
                }

                self.viewport.cancel_tool();
                self.viewport.revert_all();
            }
            ViewerMessage::Share => {}
            ViewerMessage::Print => {}
            ViewerMessage::Cancelled => {}
            ViewerMessage::Quit => {}
            ViewerMessage::ToolbarOverflowToggle => {
                self.toolbar_overflow_open = !self.toolbar_overflow_open;
            }
            ViewerMessage::KeyPressed(key, modifiers) => {
                if self.text_editing
                    && let Some(preview) = self.viewport.preview_mut()
                    && let Some(text_preview) = preview.as_any_mut().downcast_mut::<TextPreview>()
                {
                    match &key {
                        Key::Character(c) if !modifiers.control() && !modifiers.alt() => {
                            for ch in c.chars() {
                                text_preview.push_char(ch);
                            }
                        }
                        Key::Named(Named::Backspace) => text_preview.pop_char(),
                        Key::Named(Named::Space) => text_preview.push_char(' '),
                        Key::Named(Named::Enter) => {
                            self.text_editing = false;
                            self.viewport.apply_tool();
                            self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            self.viewport.set_preview(Some(Box::new(TextPreview::new(
                                self.annotate_color.0,
                                self.annotate_stroke_size * 8.0,
                            ))));
                        }
                        Key::Named(Named::Escape) => {
                            self.text_editing = false;
                            self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            self.viewport.set_preview(Some(Box::new(TextPreview::new(
                                self.annotate_color.0,
                                self.annotate_stroke_size * 8.0,
                            ))));
                        }
                        _ => {}
                    }
                } else if let Some(msg) = keyboard_shortcut_handler(key, modifiers) {
                    return self.update(msg);
                }
            }
            ViewerMessage::WindowResized(size) => self.window_width = Some(size.width),
            ViewerMessage::Nav(msg) => match msg {
                NavMessage::ScanComplete(dir, images, select) => {
                    self.viewport.cancel_tool();
                    self.viewport.set_image(None);

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
                        self.grid.set_selected(vec![idx]);

                        // Clear any tool and any unsaved tool operations
                        self.viewport.cancel_tool();
                        self.viewport.revert_all();

                        if let Some(cached) = self.cache.get_full(&path) {
                            self.viewport.set_image(Some(CanvasImage {
                                handle: cached.handle,
                                width: cached.width,
                                height: cached.height,
                            }));
                        } else {
                            self.viewport.set_image(None);
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
                            // Clear any tool that is active when reselecting a new image.
                            if self.viewport.active_tool().is_some() {
                                self.viewport.cancel_tool();
                            }

                            // Set the selected image to canvas image.
                            if let Some(cached) = self.cache.get_full(&path) {
                                self.viewport.set_image(Some(CanvasImage {
                                    handle: cached.handle,
                                    width: cached.width,
                                    height: cached.height,
                                }));
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
                    let old_zoom = self.viewport.zoom();
                    let new_zoom = (old_zoom * 1.25).min(10.0);
                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_zoom_changed(old_zoom, new_zoom, size);
                    }
                    self.viewport.set_zoom(new_zoom);
                }
                CanvasMessage::ZoomOut => {
                    let old_zoom = self.viewport.zoom();
                    let new_zoom = (old_zoom / 1.25).max(0.1);
                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_zoom_changed(old_zoom, new_zoom, size);
                    }
                    self.viewport.set_zoom(new_zoom);
                }
                CanvasMessage::Pan(pan) => self.viewport.set_pan(pan),
                CanvasMessage::FitToView => {
                    self.viewport.set_zoom(1.0);
                    self.viewport.set_pan(Vector::ZERO);
                }
                CanvasMessage::Fullscreen => {
                    self.is_fullscreen = !self.is_fullscreen;
                    if let Some(window_id) = self.core.main_window_id() {
                        return if self.is_fullscreen {
                            self.core.set_header_title(String::new());
                            cosmic::iced::window::change_mode(
                                window_id,
                                cosmic::iced::window::Mode::Fullscreen,
                            )
                        } else {
                            self.core.set_header_title(fl!("app-title"));
                            cosmic::iced::window::change_mode(
                                window_id,
                                cosmic::iced::window::Mode::Windowed,
                            )
                        };
                    }
                }
                CanvasMessage::ToolStart(point) => {
                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_press(point, size);
                        self.viewport.tool_dragging = true;
                    }
                }
                CanvasMessage::ToolDrag(point) => {
                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_drag(point, size);
                    }
                }
                CanvasMessage::ToolEnd => {
                    self.viewport.tool_dragging = false;
                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_release(Point::ORIGIN, size);
                    }

                    // Set text_editing flag when text preview enters editing mode
                    if matches!(self.annotate_tool, AnnotateTool::Text) {
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text_preview) =
                                preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            self.text_editing = text_preview.is_editing();
                        }
                    }

                    // Auto-commit for annotation, each stroke goes on the undo stack
                    if self.viewport.active_tool() == Some(ToolKind::Annotate)
                        && !matches!(self.annotate_tool, AnnotateTool::Text)
                    {
                        self.viewport.apply_tool();
                        self.viewport.set_active_tool(Some(ToolKind::Annotate));
                        let preview: Box<dyn ToolOperation> = match self.annotate_tool {
                            AnnotateTool::Highlighter => Box::new(HighlighterPreview::new(
                                self.annotate_color.0,
                                self.annotate_stroke_size,
                            )),
                            AnnotateTool::Pencil => Box::new(PencilPreview::new(
                                self.annotate_color.0,
                                self.annotate_stroke_size,
                            )),
                            AnnotateTool::Rectangle
                            | AnnotateTool::Ellipse
                            | AnnotateTool::Line
                            | AnnotateTool::Arrow
                            | AnnotateTool::Star
                            | AnnotateTool::Polygon => {
                                let kind = match self.annotate_tool {
                                    AnnotateTool::Rectangle => ShapeKind::Rectangle,
                                    AnnotateTool::Ellipse => ShapeKind::Ellipse,
                                    AnnotateTool::Line => ShapeKind::Line,
                                    AnnotateTool::Arrow => ShapeKind::Arrow,
                                    AnnotateTool::Star => ShapeKind::Star,
                                    AnnotateTool::Polygon => ShapeKind::Polygon,
                                    _ => unreachable!(),
                                };
                                Box::new(ShapePreview::new(
                                    kind,
                                    self.annotate_color.0,
                                    self.annotate_stroke_size,
                                ))
                            }
                            _ => Box::new(PenPreview::new(
                                self.annotate_color.0,
                                self.annotate_stroke_size,
                            )),
                        };
                        self.viewport.set_preview(Some(preview));
                    }
                }
            },
            ViewerMessage::Edit(msg) => {
                self.context_menu_position = None;
                match msg {
                    EditMessage::Annotate => {
                        if self.viewport.active_tool() != Some(ToolKind::Annotate) {
                            self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            self.viewport.set_preview(Some(Box::new(PenPreview::new(
                                self.annotate_color.0,
                                2.0,
                            ))));
                        }
                    }
                    EditMessage::AnnotateApply => {
                        self.viewport.set_active_tool(None);
                        self.viewport.set_preview(None);
                    }
                    EditMessage::AnnotateCancel => {
                        self.viewport.cancel_tool();
                        self.viewport.revert_all();
                    }
                    EditMessage::AnnotateStroke(size) => {
                        let sizes: [f32; 5] = [2., 4., 6., 8., 10.];
                        if let Some(&size) = sizes.get(size) {
                            self.annotate_stroke_size = size;
                            if let Some(preview) = self.viewport.preview_mut() {
                                if let Some(pen) = preview.as_any_mut().downcast_mut::<PenPreview>()
                                {
                                    pen.width = size;
                                } else if let Some(pencil) =
                                    preview.as_any_mut().downcast_mut::<PencilPreview>()
                                {
                                    pencil.width = size;
                                } else if let Some(highlighter) =
                                    preview.as_any_mut().downcast_mut::<HighlighterPreview>()
                                {
                                    highlighter.width = size;
                                } else if let Some(shape) =
                                    preview.as_any_mut().downcast_mut::<ShapePreview>()
                                {
                                    shape.width = size;
                                } else if let Some(text) =
                                    preview.as_any_mut().downcast_mut::<TextPreview>()
                                {
                                    text.font_size = size * 8.0;
                                }
                            }
                        }
                    }
                    EditMessage::AnnotateTool(tool) => {
                        self.annotate_tool = tool;
                        match tool {
                            AnnotateTool::Highlighter => {
                                self.viewport
                                    .set_preview(Some(Box::new(HighlighterPreview::new(
                                        self.annotate_color.0,
                                        self.annotate_stroke_size,
                                    ))));
                            }
                            AnnotateTool::Pen => {
                                self.viewport.set_preview(Some(Box::new(PenPreview::new(
                                    self.annotate_color.0,
                                    self.annotate_stroke_size,
                                ))));
                            }
                            AnnotateTool::Pencil => {
                                self.viewport.set_preview(Some(Box::new(PencilPreview::new(
                                    self.annotate_color.0,
                                    self.annotate_stroke_size,
                                ))));
                            }
                            AnnotateTool::Rectangle
                            | AnnotateTool::Ellipse
                            | AnnotateTool::Line
                            | AnnotateTool::Arrow
                            | AnnotateTool::Star
                            | AnnotateTool::Polygon => {
                                let kind = match tool {
                                    AnnotateTool::Rectangle => ShapeKind::Rectangle,
                                    AnnotateTool::Ellipse => ShapeKind::Ellipse,
                                    AnnotateTool::Line => ShapeKind::Line,
                                    AnnotateTool::Arrow => ShapeKind::Arrow,
                                    AnnotateTool::Star => ShapeKind::Star,
                                    AnnotateTool::Polygon => ShapeKind::Polygon,
                                    _ => unreachable!(),
                                };
                                self.viewport.set_preview(Some(Box::new(ShapePreview::new(
                                    kind,
                                    self.annotate_color.0,
                                    self.annotate_stroke_size,
                                ))));
                            }
                            AnnotateTool::Text => {
                                self.viewport.set_preview(Some(Box::new(TextPreview::new(
                                    self.annotate_color.0,
                                    self.annotate_stroke_size * 8.0,
                                ))));
                                self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            }
                        }
                    }
                    EditMessage::AnnotateColor(color) => {
                        self.annotate_color = color;

                        // Update the active preview's color if one exists
                        if let Some(preview) = self.viewport.preview_mut() {
                            if let Some(pen) = preview.as_any_mut().downcast_mut::<PenPreview>() {
                                pen.color = color.0;
                            } else if let Some(pencil) =
                                preview.as_any_mut().downcast_mut::<PencilPreview>()
                            {
                                pencil.color = color.0;
                            } else if let Some(highlighter) =
                                preview.as_any_mut().downcast_mut::<HighlighterPreview>()
                            {
                                highlighter.color = color.0;
                            } else if let Some(shape) =
                                preview.as_any_mut().downcast_mut::<ShapePreview>()
                            {
                                shape.color = color.0;
                            } else if let Some(text) =
                                preview.as_any_mut().downcast_mut::<TextPreview>()
                            {
                                text.color = color.0;
                            }
                        }
                    }
                    EditMessage::Crop => {
                        if self.viewport.active_tool() != Some(ToolKind::Crop) {
                            self.viewport.set_active_tool(Some(ToolKind::Crop));
                            let mut selection = CropSelection::new();
                            if let Some(size) = self.viewport.image_size() {
                                selection.activate(CropRatio::Custom, size);
                                self.crop_ratio = CropRatio::Custom;
                            }
                            self.viewport.set_preview(Some(Box::new(selection)));
                        }
                    }
                    EditMessage::CropApply => {
                        if self.viewport.apply_tool() {
                            self.rebuild_viewport();
                        }
                    }
                    EditMessage::CropCancel => {
                        self.viewport.cancel_tool();
                        if let Some(current) = self.nav.current() {
                            self.viewport.revert_all();
                            self.cache.remove_full(current);
                            tasks.push(self.load_full_image(current.to_path_buf()));
                        }
                    }
                    EditMessage::CropRatio(ratio) => {
                        let size = self.viewport.image_size();
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(crop) = preview.as_any_mut().downcast_mut::<CropSelection>()
                            && let Some(size) = size
                        {
                            crop.set_ratio(ratio, size);
                            self.crop_ratio = ratio;
                        }
                    }
                    EditMessage::RotateLeft => {
                        self.viewport.cancel_tool();
                        self.viewport
                            .commit(Box::new(RotateOperation::new(RotateDirection::Left)));
                        self.rebuild_viewport();
                    }
                    EditMessage::RotateRight => {
                        self.viewport.cancel_tool();
                        self.viewport
                            .commit(Box::new(RotateOperation::new(RotateDirection::Right)));
                        self.rebuild_viewport();
                    }
                    EditMessage::Undo => self.viewport.undo(),
                    EditMessage::Redo => self.viewport.redo(),
                    EditMessage::RevertAll => self.viewport.revert_all(),
                }
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
        Subscription::batch([
            on_key_press(|key, modifiers| Some(ViewerMessage::KeyPressed(key, modifiers))),
            event::listen_with(|event, _status, _id| match event {
                iced::Event::Window(iced::window::Event::Resized(size)) => {
                    Some(ViewerMessage::WindowResized(size))
                }
                _ => None,
            }),
        ])
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
        if idx > 0 && (s.len() - idx).is_multiple_of(3) {
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
