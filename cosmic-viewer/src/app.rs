use crate::{
    fl,
    key_binds::{self, MenuAction, keyboard_shortcut_handler},
    menu::menu_bar,
    message::{ContextMessage, EditMessage, ImageMessage, NavMessage, ViewerMessage},
    watcher::WatcherEvent,
};
use cosmic::{
    Action, Application, ApplicationExt, Core, Element, Task, Theme,
    app::context_drawer,
    cosmic_config::CosmicConfigEntry,
    dialog::file_chooser::{self, FileFilter},
    iced::{
        self, Alignment, Background, Border, Color, Length, Point, Rectangle, Size, Subscription,
        Vector,
        advanced::graphics::text::{cosmic_text, font_system},
        alignment::Horizontal,
        clipboard, event, font,
        keyboard::key::{Key, Named},
        widget::{
            grid,
            scrollable::{AbsoluteOffset, scroll_to},
            sensor, stack,
        },
        window,
    },
    task::future,
    theme::{self, Button},
    widget::{
        self, Column, Id, Row, Space, Toasts, button, canvas,
        color_picker::ColorPickerUpdate::{self, AppliedColor, Cancel, ToggleColorPicker},
        container, divider, dropdown, icon,
        image::Handle,
        menu::{KeyBind, menu_button},
        nav_bar, popover, scrollable,
        space::horizontal,
        text, toaster,
    },
};
use image::DynamicImage;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
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
        ShapePreview, TextDragHandle, TextOperation, TextPreview,
    },
    crop::{CropOperation, CropRatio, CropSelection},
    rotate::{RotateDirection, RotateOperation},
};
use viewer_widgets::dashed_shape::DashedBorder;

// reason: independent UI toggle flags (text styles, popups, fullscreen, prefs);
// grouping into a bitflags type would obscure their distinct meanings.
#[allow(clippy::struct_excessive_bools)]
pub struct CosmicViewer {
    core: Core,
    key_binds: HashMap<KeyBind, MenuAction>,
    config: ViewerConfig,
    nav: NavState,
    nav_bar_model: nav_bar::Model,
    cache: ImageCache,
    //grid: ImageGrid<'static, Action<ViewerMessage>>,
    nav_handles: Vec<Option<Handle>>,
    scroll_id: Id,
    viewport: ViewportManager,
    context_page: Option<ContextMessage>,
    context_menu_position: Option<Point>,
    annotate_tool: AnnotateTool,
    annotate_color: AnnotateColor,
    annotate_stroke_size: f32,
    highlighter_stroke_size: f32,
    crop_ratio: CropRatio,
    crop_ratio_popup: bool,
    text_editing: bool,
    move_mode: bool,
    move_target: Option<usize>,
    move_start: Option<Point>,
    font_families: Vec<&'static str>,
    text_font_family: &'static str,
    text_font_index: Option<usize>,
    text_font_size: f32,
    text_bold: bool,
    text_italic: bool,
    text_underline: bool,
    text_alignment: Horizontal,
    show_text_format_menu: bool,
    shape_popup: bool,
    stroke_popup: bool,
    color_picker: cosmic::widget::ColorPickerModel,
    has_custom_color: bool,
    selected_shape: AnnotateTool,
    window_width: Option<f32>,
    is_fullscreen: bool,
    saved_nav_toggled: bool,
    wallpaper_dialog: Option<PathBuf>,
    delete_dialog: Option<PathBuf>,
    available_outputs: Vec<String>,
    watcher_rescan_pending: bool,
    was_narrow: bool,
    toasts: Toasts<ViewerMessage>,
}

impl CosmicViewer {
    // reason: thumbnail dimension is a small pixel count, exact as f32.
    #[allow(clippy::cast_precision_loss)]
    fn is_narrow(&self) -> bool {
        // Tie "narrow" to libcosmic's own condensed breakpoint (the point it would hide the main
        // content, ~360px content width). A separate, narrower threshold left a gap band where the
        // framework hid the content but the app still drew the nav as a fixed column, so the main
        // view vanished (#32).
        self.core().is_condensed()
    }

    fn save_last_color(&mut self) {
        let c = self.annotate_color.0;
        self.config.last_color = Some([c.r, c.g, c.b, c.a]);
        if let Ok(config_handle) = viewer_config::config() {
            let _ = self.config.write_entry(&config_handle);
        }
    }

    fn reload_image_list(&self) -> Task<Action<ViewerMessage>> {
        let include_hidden = self.config.show_hidden_files;
        let sort_mode = self.config.sort_mode;
        let sort_order = self.config.sort_order;

        let dir = self.nav.current().map_or_else(
            || self.nav.dir().map(std::path::Path::to_path_buf),
            |current| get_image_dir(current),
        );

        if let Some(dir) = dir {
            return future(async move {
                let images = scan_dir(&dir, include_hidden, sort_mode, sort_order).await;
                Action::App(ViewerMessage::Nav(NavMessage::DirectoryRefreshed(images)))
            });
        }

        Task::none()
    }

    fn open_path(&self, path: PathBuf) -> Task<Action<ViewerMessage>> {
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

    // reason: thumbnail size and grid index are small in-range counts; exact as f32.
    #[allow(clippy::cast_precision_loss)]
    fn grid_scroll_offset(&self, idx: usize) -> f32 {
        let thumb_size = self.config.thumbnail_size.pixels() as f32;
        let button_padding = 8.0;
        let cell_size = thumb_size + (button_padding * 2.0);
        let row_spacing = 8.0;
        idx as f32 * (cell_size + row_spacing)
    }

    fn rebuild_nav_handles(&mut self) {
        self.nav_handles = self
            .nav
            .images()
            .iter()
            .map(|path| self.cache.get_thumbnail(path))
            .collect();
    }

    /// Rebuild the working image.
    /// Used after dimension-changing operations like crop and rotate.
    fn rebuild_working_image(&mut self) {
        let original = self
            .nav
            .current()
            .and_then(|path| self.cache.get_full(path))
            .map(|cached| cached.image);

        if let Some(mut base) = original {
            for op in self.viewport.operations() {
                if let Some(rotate) = op.as_any().downcast_ref::<RotateOperation>() {
                    base = match rotate.direction {
                        RotateDirection::Left => base.rotate270(),
                        RotateDirection::Right => base.rotate90(),
                    };
                } else if let Some(crop) = op.as_any().downcast_ref::<CropOperation>() {
                    base = crop_to_region(&base, crop.region);
                }
            }

            if let Some(img) = self.viewport.working_image_mut() {
                *img = base;
            }

            self.viewport.rebuild_display_preserve_view();
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
            .map(|path| {
                self.cache.set_thumbnail_pending(path.clone());
                let cache = self.cache.clone();
                let path = path.clone();
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

    fn load_full_image(&self, path: PathBuf) -> Task<Action<ViewerMessage>> {
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
        let mut content = Column::new().spacing(spacing.space_m);

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

    fn build_context_menu_element<'a>() -> Element<'a, ViewerMessage> {
        let menu_item = |label: String, message: ViewerMessage| {
            menu_button(vec![text(label).into()]).on_press(message)
        };

        container(
            Column::new()
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
                ))
                .push(menu_item(
                    fl!("menu-set-wallpaper"),
                    ViewerMessage::SetWallpaper,
                ))
                .push(menu_item(
                    fl!("menu-move-to-trash"),
                    ViewerMessage::MoveToTrash,
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

    fn popup_style(theme: &cosmic::Theme) -> container::Style {
        let cosmic = theme.cosmic();
        let component = &cosmic.background.component;
        container::Style {
            background: Some(Background::Color(component.base.into())),
            border: Border {
                radius: cosmic.radius_s().map(|x| x + 1.0).into(),
                width: 1.0,
                color: component.divider.into(),
            },
            ..Default::default()
        }
    }

    fn build_toolbar(&self) -> Element<'_, ViewerMessage> {
        match self.viewport.active_tool() {
            Some(ToolKind::Crop) => self.build_crop_toolbar(),
            Some(ToolKind::Annotate) => self.build_annotate_toolbar(),
            _ => self.build_default_toolbar(),
        }
    }

    // reason: zoom percent is rounded, non-negative, and well within u32 range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn build_default_toolbar(&self) -> Element<'_, ViewerMessage> {
        // The toolbar measures its own fit and stacks only when the single row
        // overflows; force stacking only when the window is truly condensed.
        let mode = if self.core().is_condensed() {
            ToolbarMode::Compact
        } else {
            ToolbarMode::Full
        };

        let icon_btn = |name: &'static str,
                        tooltip: String,
                        msg: ViewerMessage|
         -> Element<'_, ViewerMessage> {
            button::icon(icon::from_name(name))
                .tooltip(tooltip)
                .on_press(msg)
                .into()
        };

        let bounds = self.viewport.last_bounds().get();
        let viewport_size = Size::new(bounds.width, bounds.height);
        let zoom_pct = format!(
            "{}%",
            self.viewport.actual_percent(viewport_size).round() as u32
        );

        responsive_toolbar(mode)
            .start(
                ToolbarItem::new(icon_btn(
                    "markup-symbolic",
                    fl!("toolbar-annotate"),
                    ViewerMessage::Edit(EditMessage::Annotate),
                ))
                .priority(ItemPriority::Essential),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "image-crop-rotate-symbolic",
                    fl!("toolbar-crop"),
                    ViewerMessage::Edit(EditMessage::Crop),
                ))
                .priority(ItemPriority::Essential),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "object-rotate-left-symbolic",
                    fl!("toolbar-rotate-left"),
                    ViewerMessage::Edit(EditMessage::RotateLeft),
                ))
                .priority(ItemPriority::Essential),
            )
            .start(
                ToolbarItem::new(icon_btn(
                    "object-rotate-right-symbolic",
                    fl!("toolbar-rotate-right"),
                    ViewerMessage::Edit(EditMessage::RotateRight),
                ))
                .priority(ItemPriority::Essential),
            )
            .center(
                // Zoom out / percentage / zoom in stay together as one atom so
                // the responsive toolbar never splits them across rows.
                ToolbarItem::new(
                    Row::new()
                        .push(icon_btn(
                            "zoom-out-symbolic",
                            fl!("toolbar-zoom-out"),
                            ViewerMessage::Canvas(CanvasMessage::ZoomOut),
                        ))
                        .push(text::body(zoom_pct))
                        .push(icon_btn(
                            "zoom-in-symbolic",
                            fl!("toolbar-zoom-in"),
                            ViewerMessage::Canvas(CanvasMessage::ZoomIn),
                        ))
                        .align_y(Alignment::Center)
                        .spacing(cosmic::theme::active().cosmic().spacing.space_xxs),
                )
                .priority(ItemPriority::Essential),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "zoom-original-symbolic",
                    fl!("menu-actual-size"),
                    ViewerMessage::Canvas(CanvasMessage::ActualSize),
                ))
                .priority(ItemPriority::Essential),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "view-fit-symbolic",
                    fl!("menu-fit-to-view"),
                    ViewerMessage::Canvas(CanvasMessage::FitToView),
                ))
                .priority(ItemPriority::Essential),
            )
            .end(
                ToolbarItem::new(icon_btn(
                    "view-fullscreen-symbolic",
                    fl!("toolbar-fullscreen"),
                    ViewerMessage::Canvas(CanvasMessage::Fullscreen),
                ))
                .priority(ItemPriority::Essential),
            )
            .view()
    }

    fn build_crop_ratio_selector(&self) -> Element<'_, ViewerMessage> {
        let trigger = button::custom(
            Row::new()
                .push(icon::from_name("ratios-symbolic").size(16).icon())
                .push(icon::from_name("pan-down-symbolic").size(12).icon())
                .align_y(Alignment::Center)
                .spacing(2),
        )
        .class(cosmic::theme::Button::Icon)
        .on_press(ViewerMessage::Edit(EditMessage::CropRatioPopupToggle));

        let mut pop = popover(trigger);

        if self.crop_ratio_popup {
            let is_portrait = self
                .viewport
                .image_size()
                .is_some_and(|size| size.height > size.width);

            let presets = CropRatio::presets();
            let mut list = Column::new().spacing(4);

            for ratio in presets {
                let label = ratio.label(is_portrait).to_string();
                let is_selected = *ratio == self.crop_ratio;
                let item = Row::new()
                    .push(container(text::body(label)).width(Length::Fixed(72.0)))
                    .push(if is_selected {
                        Element::from(icon(
                            icon::from_name("object-select-symbolic").size(16).into(),
                        ))
                    } else {
                        Element::from(Space::new().width(16))
                    })
                    .align_y(Alignment::Center)
                    .width(Length::Fixed(104.0))
                    .spacing(8);

                list = list
                    .push(
                        button::custom(item)
                            .class(theme::Button::Icon)
                            .width(Length::Fixed(104.0))
                            .on_press(ViewerMessage::Edit(EditMessage::CropRatio(*ratio))),
                    )
                    .width(Length::Shrink);
            }

            let popup = container(list).padding(8).style(|theme| {
                let cosmic = theme.cosmic();
                let component = &cosmic.background.component;
                container::Style {
                    icon_color: None,
                    text_color: None,
                    background: Some(Background::Color(component.base.into())),
                    border: Border {
                        radius: cosmic.radius_s().map(|x| x + 1.0).into(),
                        width: 1.0,
                        color: component.divider.into(),
                    },
                    ..Default::default()
                }
            });

            pop = pop
                .popup(popup)
                // NOTE: libcosmic has no Position::Top, so this Point offset places the popup
                // on top of the toolbar, centered on the button. Revisit if upstream adds it.
                .position(popover::Position::Point(Point::new(-35.0, -10.0)))
                .on_close(ViewerMessage::Edit(EditMessage::CropRatioPopupToggle));
        }

        pop.into()
    }

    fn build_crop_toolbar(&self) -> Element<'_, ViewerMessage> {
        // The toolbar measures its own fit and stacks only when the single row
        // overflows; force stacking only when the window is truly condensed.
        let mode = if self.core().is_condensed() {
            ToolbarMode::Compact
        } else {
            ToolbarMode::Full
        };

        let icon_btn = |name: &'static str,
                        tooltip: String,
                        msg: ViewerMessage|
         -> Element<'_, ViewerMessage> {
            button::icon(icon::from_name(name))
                .tooltip(tooltip)
                .on_press(msg)
                .into()
        };

        let can_undo = self.viewport.can_undo();
        let can_redo = self.viewport.can_redo();

        let undo_btn = button::icon(icon::from_name("edit-undo-symbolic"))
            .tooltip(fl!("menu-undo"))
            .on_press_maybe(can_undo.then(|| ViewerMessage::Edit(EditMessage::Undo)));

        let redo_btn = button::icon(icon::from_name("edit-redo-symbolic"))
            .tooltip(fl!("menu-redo"))
            .on_press_maybe(can_redo.then(|| ViewerMessage::Edit(EditMessage::Redo)));

        let zoom_pct = {
            let bounds = self.viewport.last_bounds().get();
            format!(
                "{:.0}%",
                self.viewport
                    .actual_percent(Size::new(bounds.width, bounds.height))
            )
        };

        responsive_toolbar(mode)
            .start(ToolbarItem::new(undo_btn))
            .start(ToolbarItem::new(redo_btn))
            .start(ToolbarItem::new(
                divider::vertical::light().height(Length::Fixed(32.0)),
            ))
            .start(ToolbarItem::new(icon_btn(
                "object-rotate-left-symbolic",
                fl!("menu-rotate-left"),
                ViewerMessage::Edit(EditMessage::RotateLeft),
            )))
            .start(ToolbarItem::new(icon_btn(
                "object-rotate-right-symbolic",
                fl!("menu-rotate-right"),
                ViewerMessage::Edit(EditMessage::RotateRight),
            )))
            .start(ToolbarItem::new(
                divider::vertical::light().height(Length::Fixed(32.0)),
            ))
            .start(ToolbarItem::new(self.build_crop_ratio_selector()))
            .center(
                // Zoom out / percentage / zoom in stay together as one atom so
                // the responsive toolbar never splits them across rows.
                ToolbarItem::new(
                    Row::new()
                        .push(icon_btn(
                            "zoom-out-symbolic",
                            fl!("toolbar-zoom-out"),
                            ViewerMessage::Canvas(CanvasMessage::ZoomOut),
                        ))
                        .push(text::body(zoom_pct))
                        .push(icon_btn(
                            "zoom-in-symbolic",
                            fl!("toolbar-zoom-in"),
                            ViewerMessage::Canvas(CanvasMessage::ZoomIn),
                        ))
                        .align_y(Alignment::Center)
                        .spacing(cosmic::theme::active().cosmic().spacing.space_xxs),
                )
                .priority(ItemPriority::Essential),
            )
            .end(ToolbarItem::new(
                button::standard(fl!("toolbar-cancel"))
                    .on_press(ViewerMessage::Edit(EditMessage::CropCancel)),
            ))
            .end(ToolbarItem::new(
                button::icon(icon::from_name("object-select-symbolic"))
                    .tooltip(fl!("toolbar-apply"))
                    .on_press(ViewerMessage::Edit(EditMessage::CropApply)),
            ))
            .view()
    }

    // reason: single declarative widget tree; splitting would fragment the layout.
    #[allow(clippy::too_many_lines)]
    fn build_annotate_toolbar(&self) -> Element<'_, ViewerMessage> {
        let mode = if self.core().is_condensed() {
            ToolbarMode::Compact
        } else {
            ToolbarMode::Full
        };

        let icon_btn = |name: &'static str,
                        tooltip: String,
                        msg: ViewerMessage|
         -> button::IconButton<ViewerMessage> {
            button::icon(icon::from_name(name))
                .tooltip(tooltip)
                .on_press(msg)
        };

        let can_undo = self.viewport.can_undo();
        let can_redo = self.viewport.can_redo();

        let undo_btn = button::icon(icon::from_name("edit-undo-symbolic"))
            .tooltip(fl!("menu-undo"))
            .on_press_maybe(can_undo.then(|| ViewerMessage::Edit(EditMessage::Undo)));

        let redo_btn = button::icon(icon::from_name("edit-redo-symbolic"))
            .tooltip(fl!("menu-redo"))
            .on_press_maybe(can_redo.then(|| ViewerMessage::Edit(EditMessage::Redo)));

        let mut toolbar = responsive_toolbar(mode)
            .start(ToolbarItem::new(undo_btn))
            .start(ToolbarItem::new(redo_btn))
            .start(ToolbarItem::new(
                divider::vertical::light().height(Length::Fixed(32.0)),
            ))
            .start(ToolbarItem::new(
                icon_btn(
                    "insert-text-symbolic",
                    fl!("text-tool"),
                    ViewerMessage::Edit(EditMessage::AnnotateTool(AnnotateTool::Text)),
                )
                .selected(self.annotate_tool == AnnotateTool::Text)
                .class(tool_toggle_class(self.annotate_tool == AnnotateTool::Text)),
            ))
            .start(ToolbarItem::new(
                icon_btn(
                    "insert-drawing-symbolic",
                    fl!("drawing-pen"),
                    ViewerMessage::Edit(EditMessage::AnnotateTool(AnnotateTool::draw_tools()[0])),
                )
                .selected(self.annotate_tool == AnnotateTool::draw_tools()[0])
                .class(tool_toggle_class(
                    self.annotate_tool == AnnotateTool::draw_tools()[0],
                )),
            ))
            .start(ToolbarItem::new(
                icon_btn(
                    "text-highlight-symbolic",
                    fl!("drawing-highlighter"),
                    ViewerMessage::Edit(EditMessage::AnnotateTool(AnnotateTool::Highlighter)),
                )
                .selected(self.annotate_tool == AnnotateTool::Highlighter)
                .class(tool_toggle_class(
                    self.annotate_tool == AnnotateTool::Highlighter,
                )),
            ))
            .start(ToolbarItem::new(self.build_shape_selector()).width_hint(120.0))
            .start(ToolbarItem::new({
                let has_movable =
                    self.viewport.operations().iter().any(|op| op.movable()) && !self.text_editing;
                let mut btn = button::icon(icon::from_name("object-move-symbolic"))
                    .tooltip(fl!("toolbar-move"));
                if self.move_mode {
                    btn = btn.class(tool_toggle_class(self.move_mode));
                }
                let el: Element<'_, ViewerMessage> = btn
                    .on_press_maybe(
                        has_movable.then(|| ViewerMessage::Edit(EditMessage::ToggleMoveMode)),
                    )
                    .into();
                el
            }))
            .center(
                ToolbarItem::new(if matches!(self.annotate_tool, AnnotateTool::Text) {
                    self.build_text_format_dropdown()
                } else {
                    self.build_stroke_selector()
                })
                .width_hint(140.0),
            );

        let accent: Color = cosmic::theme::active().cosmic().accent_color().into();
        let swatch_size = 14.0;

        let colors = AnnotateColor::presets();
        for color in &colors {
            let c = *color;
            let is_selected = c == self.annotate_color;
            toolbar = toolbar.center(ToolbarItem::new(
                button::custom(
                    container(Space::new().width(swatch_size).height(swatch_size)).class(
                        cosmic::theme::Container::custom(move |_theme| container::Style {
                            background: Some(Background::Color(c.0)),
                            border: Border {
                                radius: (swatch_size / 2.0).into(),
                                width: if is_selected { 2.0 } else { 0.0 },
                                color: accent,
                            },
                            ..Default::default()
                        }),
                    ),
                )
                .padding(2)
                .class(cosmic::theme::Button::Icon)
                .on_press(ViewerMessage::Edit(EditMessage::AnnotateColor(c))),
            ));
        }

        // Custom color picker button
        let custom_color = self
            .color_picker
            .get_applied_color()
            .unwrap_or(Color::BLACK);
        let is_custom_selected = !colors.contains(&self.annotate_color);

        let inner: Element<'_, ViewerMessage> = if is_custom_selected {
            let filled = container(Space::new().width(swatch_size).height(swatch_size)).class(
                theme::Container::custom(move |_theme| container::Style {
                    background: Some(Background::Color(custom_color)),
                    border: Border {
                        radius: (swatch_size / 2.0).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            );

            let dashed_accent = DashedBorder::circle(accent, 2.0).dash_pattern(3.0, 3.0);

            let border_layer = canvas(dashed_accent)
                .width(Length::Fixed(swatch_size))
                .height(Length::Fixed(swatch_size));

            stack!()
                .push(filled)
                .push(border_layer)
                .width(Length::Fixed(swatch_size))
                .height(Length::Fixed(swatch_size))
                .into()
        } else {
            let neutral: Color = theme::active().cosmic().palette.neutral_5.into();
            let dashed = DashedBorder::circle(neutral, 2.0).dash_pattern(3.0, 3.0);

            let canvas_bg = canvas(dashed)
                .width(Length::Fixed(swatch_size))
                .height(Length::Fixed(swatch_size));

            let add_icon = container(icon::from_name("list-add-symbolic").size(10).icon())
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill);

            stack!()
                .push(canvas_bg)
                .push(add_icon)
                .width(Length::Fixed(swatch_size))
                .height(Length::Fixed(swatch_size))
                .into()
        };

        let color_picker_btn = button::custom(inner)
            .padding(2)
            .class(theme::Button::Icon)
            .on_press(ViewerMessage::Edit(EditMessage::ColorPicker(
                ToggleColorPicker,
            )));

        let mut color_picker_popover = popover(color_picker_btn);
        if self.color_picker.get_is_active() {
            let picker = self
                .color_picker
                .builder(|u| ViewerMessage::Edit(EditMessage::ColorPicker(u)))
                .reset_label("Reset to default")
                .build("Recent colors", "Copy", "Copied!");

            let popup = container(picker)
                .padding(theme::active().cosmic().spacing.space_s)
                .max_width(260.0)
                .style(|theme| {
                    let cosmic = theme.cosmic();
                    let component = &cosmic.background.component;
                    container::Style {
                        background: Some(Background::Color(component.base.into())),
                        border: Border {
                            radius: cosmic.radius_s().map(|x| x + 1.0).into(),
                            width: 1.0,
                            color: component.divider.into(),
                        },
                        ..Default::default()
                    }
                });

            let popup = widget::mouse_area(popup).on_press(ViewerMessage::Cancelled);

            color_picker_popover = color_picker_popover
                .popup(popup)
                .position(popover::Position::Point(Point::new(-110.0, 0.0)))
                .on_close(ViewerMessage::Edit(EditMessage::ColorPicker(
                    ToggleColorPicker,
                )));
        }

        toolbar = toolbar.center(ToolbarItem::new(color_picker_popover));

        toolbar = toolbar.end(
            ToolbarItem::new(
                button::standard(fl!("toolbar-cancel"))
                    .on_press(ViewerMessage::Edit(EditMessage::AnnotateCancel)),
            )
            .width_hint(96.0),
        );

        toolbar = toolbar.end(ToolbarItem::new(
            button::icon(icon::from_name("object-select-symbolic"))
                .tooltip(fl!("toolbar-apply"))
                .on_press(ViewerMessage::Edit(EditMessage::AnnotateApply)),
        ));

        toolbar.view()
    }

    fn flatten_image(&self) -> Option<DynamicImage> {
        if let Some(working) = self.viewport.working_image() {
            // `working_image` already has the geometric ops (rotate/crop) baked in by
            // `rebuild_working_image`/`CropApply`. Re-applying them here would double-crop or
            // double-rotate the saved file, so flatten only the overlay (annotation) ops on top.
            let mut image = working.clone();
            for op in self.viewport.operations() {
                if op.as_any().is::<RotateOperation>() || op.as_any().is::<CropOperation>() {
                    continue;
                }
                op.apply(&mut image);
            }
            Some(image)
        } else {
            // No baked working image (nothing edited): flatten every op onto the cached original.
            let current = self.nav.current()?;
            let mut image = self.cache.get_full(current)?.image;
            for op in self.viewport.operations() {
                op.apply(&mut image);
            }
            Some(image)
        }
    }

    fn build_shape_selector(&self) -> Element<'_, ViewerMessage> {
        let current_icon = self.selected_shape.icon_name();

        let mut trigger = button::custom(
            Row::new()
                .push(icon::from_name(current_icon).size(16).icon())
                .push(icon::from_name("pan-down-symbolic").size(12).icon())
                .align_y(Alignment::Center)
                .spacing(2),
        )
        .selected(self.annotate_tool.icon_name() == current_icon)
        .class(tool_toggle_class(
            self.annotate_tool.icon_name() == current_icon,
        ));

        if self.shape_popup {
            trigger = trigger.on_press(ViewerMessage::Edit(EditMessage::ShapePopupClose));
        } else {
            trigger = trigger.on_press(ViewerMessage::Edit(EditMessage::ShapePopupOpen));
        }

        let mut pop = popover(trigger);

        if self.shape_popup {
            let make_btn = |tool: AnnotateTool, label: String| -> Element<'_, ViewerMessage> {
                let is_selected = tool == self.selected_shape;
                let item = Row::new()
                    .push((icon::from_name(tool.icon_name())).size(16).icon())
                    .push(text::body(label))
                    .push(if is_selected {
                        Element::from(icon::from_name("object-select-symbolic").size(16).icon())
                    } else {
                        Element::from(horizontal().width(16))
                    })
                    .align_y(Alignment::Center)
                    .spacing(8)
                    .width(Length::Shrink);

                button::custom(item)
                    .class(Button::Icon)
                    .width(Length::Shrink)
                    .on_press(ViewerMessage::Edit(EditMessage::AnnotateTool(tool)))
                    .into()
            };

            let list = Column::new()
                .push(make_btn(AnnotateTool::Rectangle, fl!("shapes-rectangle")))
                .push(make_btn(AnnotateTool::Ellipse, fl!("shapes-ellipse")))
                .push(make_btn(AnnotateTool::Arrow, fl!("shapes-arrow")))
                .push(make_btn(AnnotateTool::Line, fl!("shapes-line")))
                .push(make_btn(AnnotateTool::Star, fl!("shapes-star")))
                .push(make_btn(AnnotateTool::Polygon, fl!("shapes-polygon")))
                .spacing(2);

            let popup = container(list).padding(8).style(|theme| {
                let cosmic = theme.cosmic();
                let component = &cosmic.background.component;
                container::Style {
                    icon_color: None,
                    text_color: None,
                    background: Some(Background::Color(component.base.into())),
                    border: Border {
                        radius: cosmic.radius_s().map(|x| x + 1.0).into(),
                        width: 1.0,
                        color: component.divider.into(),
                    },
                    ..Default::default()
                }
            });

            pop = pop
                .popup(popup)
                .position(popover::Position::Center)
                .on_close(ViewerMessage::Edit(EditMessage::ShapePopupClose));
        }

        pop.into()
    }

    // reason: single declarative widget tree; splitting would fragment the layout.
    #[allow(clippy::too_many_lines)]
    fn build_text_format_dropdown(&self) -> Element<'_, ViewerMessage> {
        let trigger = button::custom(
            Row::new()
                .push(text("Aa"))
                .push(icon::from_name("pan-down-symbolic").size(12).icon())
                .align_y(Alignment::Center)
                .spacing(2)
                .height(Length::Fixed(16.0)),
        )
        .class(theme::Button::Icon)
        .on_press(ViewerMessage::Edit(EditMessage::ToggleTextFormatMenu));

        let mut format_popover = popover(trigger);

        if self.show_text_format_menu {
            let font_sizes = vec!["12pt", "16pt", "20pt", "24pt", "32pt", "48pt", "64pt"];
            let font_size_values = [12.0_f32, 16.0, 20.0, 24.0, 32.0, 48.0, 64.0];
            // presets are exact integer-valued floats; match the active size to one.
            let font_size_selected = font_size_values
                .iter()
                .position(|s| (*s - self.text_font_size).abs() < f32::EPSILON);

            let font_row = Row::new()
                .push(dropdown(&self.font_families, self.text_font_index, |idx| {
                    ViewerMessage::Edit(EditMessage::TextFontFamily(idx))
                }))
                .push(dropdown(font_sizes, font_size_selected, |idx| {
                    ViewerMessage::Edit(EditMessage::TextFontSize(idx))
                }))
                .align_y(Alignment::Center)
                .spacing(4);

            let style_row = Row::new()
                .push(
                    button::icon(icon::from_name("format-text-bold-symbolic"))
                        .selected(self.text_bold)
                        .class(tool_toggle_class(self.text_bold))
                        .on_press(ViewerMessage::Edit(EditMessage::TextBold)),
                )
                .push(
                    button::icon(icon::from_name("format-text-italic-symbolic"))
                        .selected(self.text_italic)
                        .class(tool_toggle_class(self.text_italic))
                        .on_press(ViewerMessage::Edit(EditMessage::TextItalic)),
                )
                .push(
                    button::icon(icon::from_name("format-text-underline-symbolic"))
                        .selected(self.text_underline)
                        .class(tool_toggle_class(self.text_underline))
                        .on_press(ViewerMessage::Edit(EditMessage::TextUnderline)),
                )
                .push(Space::new().width(8))
                .push(
                    button::icon(icon::from_name("format-justify-left-symbolic"))
                        .selected(self.text_alignment == Horizontal::Left)
                        .class(tool_toggle_class(self.text_alignment == Horizontal::Left))
                        .on_press(ViewerMessage::Edit(EditMessage::TextAlignment(
                            Horizontal::Left,
                        ))),
                )
                .push(
                    button::icon(icon::from_name("format-justify-center-symbolic"))
                        .selected(self.text_alignment == Horizontal::Center)
                        .class(tool_toggle_class(self.text_alignment == Horizontal::Center))
                        .on_press(ViewerMessage::Edit(EditMessage::TextAlignment(
                            Horizontal::Center,
                        ))),
                )
                .push(
                    button::icon(icon::from_name("format-justify-right-symbolic"))
                        .selected(self.text_alignment == Horizontal::Right)
                        .class(tool_toggle_class(self.text_alignment == Horizontal::Right))
                        .on_press(ViewerMessage::Edit(EditMessage::TextAlignment(
                            Horizontal::Right,
                        ))),
                )
                .spacing(2);

            let popup = container(
                Column::new()
                    .push(font_row)
                    .push(style_row)
                    .spacing(6)
                    .padding(8),
            )
            .style(Self::popup_style)
            .width(Length::Shrink);

            format_popover = format_popover
                .popup(popup)
                .position(popover::Position::Bottom)
                .on_close(ViewerMessage::Edit(EditMessage::ToggleTextFormatMenu));
        }

        format_popover.into()
    }

    fn build_stroke_selector(&self) -> Element<'_, ViewerMessage> {
        let trigger = button::custom(
            Row::new()
                .push(icon::from_name("stroke-width-symbolic").size(16).icon())
                .push(icon::from_name("pan-down-symbolic").size(12).icon())
                .align_y(Alignment::Center)
                .spacing(2),
        )
        .class(cosmic::theme::Button::Icon)
        .on_press(ViewerMessage::Edit(EditMessage::StrokePopupToggle));

        let mut pop = popover(trigger);

        if self.stroke_popup {
            let (labels, sizes, current) = if self.annotate_tool == AnnotateTool::Highlighter {
                (
                    &["8px", "10px", "12px", "14px", "16px", "18px", "20px"][..],
                    &[8_f32, 10., 12., 14., 16., 18., 20.][..],
                    self.highlighter_stroke_size,
                )
            } else {
                (
                    &["2px", "4px", "6px", "8px", "10px", "12px"][..],
                    &[2_f32, 4., 6., 8., 10., 12.][..],
                    self.annotate_stroke_size,
                )
            };

            let mut list = Column::new().spacing(4);
            for (label, &size) in labels.iter().zip(sizes.iter()) {
                // presets are exact integer-valued floats; match the active size.
                let is_selected = (size - current).abs() < f32::EPSILON;
                let item: Element<'_, ViewerMessage> = Row::new()
                    .push(text::body(*label))
                    .push(if is_selected {
                        Element::from(icon::from_name("object-select-symbolic").size(16).icon())
                    } else {
                        Element::from(Space::new().width(16))
                    })
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .into();

                // `size` is drawn from `sizes`, so a matching preset always exists.
                let idx = sizes
                    .iter()
                    .position(|s| (*s - size).abs() < f32::EPSILON)
                    .expect("iterated size is one of the presets");
                list = list.push(
                    button::custom(item)
                        .width(Length::Fill)
                        .class(cosmic::theme::Button::Icon)
                        .on_press(ViewerMessage::Edit(EditMessage::AnnotateStroke(idx))),
                );
            }

            let popup = container(list)
                .padding(8)
                .width(Length::Fixed(100.0))
                .style(|theme| {
                    let cosmic = theme.cosmic();
                    let component = &cosmic.background.component;
                    container::Style {
                        icon_color: None,
                        text_color: None,
                        background: Some(Background::Color(component.base.into())),
                        border: Border {
                            radius: cosmic.radius_s().map(|x| x + 1.0).into(),
                            width: 1.0,
                            color: component.divider.into(),
                        },
                        ..Default::default()
                    }
                });

            pop = pop
                .popup(popup)
                .position(popover::Position::Point(Point::new(-35.0, 0.0)))
                .on_close(ViewerMessage::Edit(EditMessage::StrokePopupToggle));
        }

        pop.into()
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

        let families = load_font_families();
        let default_family = match cosmic::font::default().family {
            font::Family::Name(name) => name,
            _ => "Sans",
        };
        let font_index = families.iter().position(|&fam| fam == default_family);

        let initial_color = config
            .last_color
            .map(|c| Color::from_rgba(c[0], c[1], c[2], c[3]));

        let viewer = Self {
            core,
            key_binds: key_binds::init_keybinds(),
            config,
            nav: NavState::new(),
            nav_bar_model: nav_bar::Model::default(),
            cache: ImageCache::with_defaults(),
            //grid,
            nav_handles: Vec::new(),
            scroll_id,
            viewport: ViewportManager::default(),
            context_page: None,
            context_menu_position: None,
            annotate_tool: AnnotateTool::default(),
            annotate_color: initial_color.map(AnnotateColor).unwrap_or_default(),
            annotate_stroke_size: 2.,
            highlighter_stroke_size: 8.,
            crop_ratio: CropRatio::Custom,
            crop_ratio_popup: false,
            text_editing: false,
            move_mode: false,
            move_target: None,
            move_start: None,
            font_families: load_font_families(),
            shape_popup: false,
            stroke_popup: false,
            color_picker: cosmic::widget::ColorPickerModel::new(
                "Hex",
                "RGB",
                None,
                initial_color.or(Some(Color::BLACK)),
            )
            .width(Length::Fixed(248.0))
            .height(Length::Fixed(148.0)),
            has_custom_color: false,
            selected_shape: AnnotateTool::Rectangle,
            text_font_family: default_family,
            text_font_index: font_index,
            text_font_size: 24.0,
            text_bold: false,
            text_italic: false,
            text_underline: false,
            text_alignment: Horizontal::Left,
            show_text_format_menu: false,
            window_width: Some(0.0),
            is_fullscreen: false,
            saved_nav_toggled: false,
            wallpaper_dialog: None,
            delete_dialog: None,
            available_outputs: Vec::new(),
            watcher_rescan_pending: false,
            was_narrow: false,
            toasts: Toasts::new(ViewerMessage::CloseToast),
        };

        if let Some(path) = flags {
            tasks.push(viewer.open_path(path));
        }

        (viewer, Task::batch(tasks))
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![menu_bar(
            &self.core,
            &self.key_binds,
            &[],
            self.viewport.can_undo(),
            self.viewport.can_redo(),
        )]
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

    // reason: thumbnail size is a small in-range pixel count; exact as f32.
    #[allow(clippy::cast_precision_loss)]
    fn nav_bar(&self) -> Option<Element<'_, Action<Self::Message>>> {
        if !self.core().nav_bar_active() {
            return None;
        }

        let thumbnail_size = self.config.thumbnail_size.pixels();
        //let col_spacing: f32 = 8.0;
        let space_xxs = f32::from(theme::active().cosmic().spacing.space_xxs);
        //let panel_width = thumbnail_size as f32 + col_spacing * 2.0 + 36.;
        let panel_width = space_xxs.mul_add(2.0, thumbnail_size as f32) + 36.0;

        let active = self.nav.index().unwrap_or(0);

        let items = self
            .nav
            .images()
            .iter()
            .enumerate()
            .map(|(img, _)| {
                let handle = self
                    .nav_handles
                    .get(img)
                    .and_then(std::clone::Clone::clone)
                    .unwrap_or_else(|| Handle::from_rgba(1, 1, vec![0, 0, 0, 0]));

                let btn = button::image(handle)
                    .selected(img == active)
                    .height(Length::Fixed(thumbnail_size as f32))
                    .width(Length::Fixed(thumbnail_size as f32))
                    .on_press(Action::App(ViewerMessage::Nav(NavMessage::GridActivate(
                        img,
                    ))));

                sensor(btn)
                    .on_show(move |_| {
                        Action::App(ViewerMessage::Nav(NavMessage::NavThumbnailShow(img)))
                    })
                    .on_hide(Action::App(ViewerMessage::Nav(
                        NavMessage::NavThumbnailHide(img),
                    )))
                    //.anticipate(thumbnail_size as f32 * 2.0)
                    .into()
            })
            .collect::<Vec<Element<'_, Action<ViewerMessage>>>>();

        let nav_grid = grid(items)
            .columns(1)
            .height(Length::Shrink)
            .spacing(space_xxs);

        let scrollable =
            scrollable(container(nav_grid).padding(space_xxs)).id(self.scroll_id.clone());

        let nav_width = if self.is_narrow() {
            // Narrow mode: overlay the view, filling window width minus a space_xxs gutter.
            self.window_width.map_or(Length::Fill, |w| {
                Length::Fixed((w - space_xxs).max(panel_width))
            })
        } else {
            Length::Fixed(panel_width)
        };

        Some(
            container(scrollable)
                .width(nav_width)
                .height(Length::Fill)
                .class(theme::Container::custom(nav_bar::nav_bar_style))
                .into(),
        )
    }

    // reason: top-level declarative view tree; splitting would fragment the layout.
    #[allow(clippy::too_many_lines)]
    fn view(&self) -> Element<'_, Self::Message> {
        let has_image = self.viewport.image().is_some();
        let content: Element<'_, Self::Message> = if has_image {
            self.viewport.element().map(ViewerMessage::Canvas)
        } else {
            Column::new()
                .push(Space::new().height(Length::Fill))
                .push(text("No image selected").center())
                .push(Space::new().height(Length::Fill))
                .align_x(Horizontal::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        let spacing = cosmic::theme::active().cosmic().spacing;

        let nav_collapsed = !self.core().nav_bar_active();
        let cur_idx = self.nav.index().unwrap_or(0);

        let image_area: Element<'_, Self::Message> = if nav_collapsed && has_image {
            let has_prev = cur_idx > 0;
            let has_next = cur_idx + 1 < self.nav.total();

            let nav_btn = |icon_name: &'static str,
                           msg: ViewerMessage,
                           enabled: bool|
             -> Element<'_, Self::Message> {
                let mut btn = button::icon(icon::from_name(icon_name).size(24))
                    .class(cosmic::theme::Button::Icon);
                if enabled {
                    btn = btn.on_press(msg);
                }
                container(btn).center_y(Length::Fill).into()
            };

            Row::new()
                .push(nav_btn(
                    "go-previous-symbolic",
                    ViewerMessage::Nav(NavMessage::GridActivate(cur_idx.saturating_sub(1))),
                    has_prev,
                ))
                .push(container(content).width(Length::Fill).height(Length::Fill))
                .push(nav_btn(
                    "go-next-symbolic",
                    ViewerMessage::Nav(NavMessage::GridActivate(
                        (cur_idx + 1).min(self.nav.total().saturating_sub(1)),
                    )),
                    has_next,
                ))
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        let main = Column::new()
            .push(image_area)
            .push(
                container(self.build_toolbar())
                    .center_x(Length::Fill)
                    .padding([spacing.space_xxs, 0, spacing.space_xxs, 0]),
            )
            .width(Length::Fill)
            .height(Length::Fill);

        let mut pop = widget::popover(main);
        if let Some(point) = self.context_menu_position
            && self.wallpaper_dialog.is_none()
            && self.delete_dialog.is_none()
        {
            pop = pop
                .popup(Self::build_context_menu_element())
                .position(widget::popover::Position::Point(point))
                .on_close(ViewerMessage::Canvas(CanvasMessage::ContextMenu(None)));
        }

        let view: Element<'_, Self::Message> = pop.into();

        if let Some(ref path) = self.wallpaper_dialog {
            let path = path.clone();
            let spacing = cosmic::theme::active().cosmic().spacing;

            let mut btn_col = Column::new().spacing(spacing.space_s);
            btn_col = btn_col.push(button::standard(fl!("wallpaper-all-displays")).on_press(
                ViewerMessage::SetWallpaperOn(path.clone(), crate::message::WallpaperTarget::All),
            ));
            for output in &self.available_outputs {
                btn_col = btn_col.push(button::standard(output.clone()).on_press(
                    ViewerMessage::SetWallpaperOn(
                        path.clone(),
                        crate::message::WallpaperTarget::Output(output.clone()),
                    ),
                ));
            }

            let dialog: Element<'_, Self::Message> = container(
                container(
                    Column::new()
                        .push(text::title4(fl!("wallpaper-dialog-title")))
                        .push(btn_col)
                        .push(
                            button::text(fl!("wallpaper-cancel"))
                                .on_press(ViewerMessage::CloseWallpaperDialog),
                        )
                        .spacing(spacing.space_s)
                        .align_x(Alignment::Center),
                )
                .padding(spacing.space_m)
                .class(cosmic::theme::Container::Dialog),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();

            let backdrop = cosmic::widget::mouse_area(
                container(Space::new().width(Length::Fill).height(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .class(cosmic::theme::Container::Transparent),
            )
            .on_press(ViewerMessage::CloseWallpaperDialog);

            return toaster(&self.toasts, stack![view, backdrop, dialog]);
        }

        toaster(&self.toasts, view)
    }

    // reason: central message dispatch; one match arm per variant, kept colocated.
    #[allow(clippy::too_many_lines)]
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
            // No-op messages: reserved menu actions with no current behavior.
            ViewerMessage::Cut
            | ViewerMessage::Paste
            | ViewerMessage::CloseFile
            | ViewerMessage::Cancelled => {}
            ViewerMessage::OpenFileDialog => {
                return future(async move {
                    let dialog = file_chooser::open::Dialog::new()
                        .title("Open Image")
                        .filter(
                            FileFilter::new("All")
                                .extension("jpg")
                                .extension("JPG")
                                .extension("jpeg")
                                .extension("JPEG")
                                .extension("png")
                                .extension("PNG")
                                .extension("gif")
                                .extension("GIF")
                                .extension("webp")
                                .extension("WEBP")
                                .extension("bmp")
                                .extension("BMP")
                                .extension("tiff")
                                .extension("TIFF")
                                .extension("tif")
                                .extension("TIF")
                                .extension("ico")
                                .extension("ICO")
                                .extension("avif")
                                .extension("AVIF")
                                .extension("hdr")
                                .extension("HDR"),
                        )
                        .filter(
                            FileFilter::new("JPEG")
                                .extension("jpg")
                                .extension("jpeg")
                                .extension("JPEG")
                                .extension("JPG"),
                        )
                        .filter(FileFilter::new("png").extension("png").extension("PNG"))
                        .filter(FileFilter::new("gif").extension("gif").extension("GIF"))
                        .filter(FileFilter::new("webp").extension("webp").extension("WEBP"))
                        .filter(FileFilter::new("bmp").extension("bmp").extension("BMP"))
                        .filter(
                            FileFilter::new("tiff")
                                .extension("tiff")
                                .extension("tif")
                                .extension("TIFF")
                                .extension("TIF"),
                        )
                        .filter(FileFilter::new("avif").extension("avif").extension("AVIF"))
                        .filter(FileFilter::new("ico").extension("ico").extension("ICO"))
                        .filter(FileFilter::new("hdr").extension("hdr").extension("HDR"));

                    match dialog.open_file().await {
                        Ok(response) => response.url().to_file_path().map_or_else(
                            |()| Action::App(ViewerMessage::Cancelled),
                            |path| Action::App(ViewerMessage::Open(path)),
                        ),
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
                        Ok(response) => response.url().to_file_path().map_or_else(
                            |()| Action::App(ViewerMessage::Cancelled),
                            |path| Action::App(ViewerMessage::Open(path)),
                        ),
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
                    tracing::error!("Failed to open containing folder: {e}");
                }
            }
            ViewerMessage::Save => {
                if let (Some(image), Some(path)) = (self.flatten_image(), self.nav.current()) {
                    if let Err(e) = image.save(path) {
                        tracing::error!("Failed to save image: {e}");
                    } else {
                        self.viewport.revert_all();
                        self.cache.remove_full(path);
                        self.viewport.cancel_tool();
                        self.text_editing = false;
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
                        Ok(response) => response
                            .url()
                            .expect("save dialog response carries a valid URL")
                            .to_file_path()
                            .map_or_else(
                                |()| Action::App(ViewerMessage::Cancelled),
                                |path| Action::App(ViewerMessage::SavedAs(path)),
                            ),
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
            ViewerMessage::Quit => {
                if let Some(id) = self.core.main_window_id() {
                    return iced::window::close(id);
                }
            }
            ViewerMessage::TextPaste(text) => {
                if self.text_editing && !text.is_empty() {
                    if let Some(preview) = self.viewport.preview_mut()
                        && let Some(t) = preview.as_any_mut().downcast_mut::<TextPreview>()
                    {
                        t.insert_with_attrs(&text);
                    }
                    self.viewport.rebuild_display();
                }
            }
            ViewerMessage::WatcherEvent(evt) => {
                match &evt {
                    WatcherEvent::Modified(path) if path.exists() => {
                        self.cache.remove_full(path);
                        self.cache.remove_thumbnail(path);
                    }
                    WatcherEvent::Modified(path) | WatcherEvent::Removed(path) => {
                        self.cache.clear_pending(path);
                        self.cache.clear_pending_thumbnail(path);
                        self.cache.remove_full(path);
                        self.cache.remove_thumbnail(path);
                        if self.nav.current() == Some(path) {
                            self.nav.deselect();
                        }
                    }
                    WatcherEvent::Created(_) => {}
                    WatcherEvent::Error(err) => {
                        tracing::warn!("watcher error: {err}");
                    }
                }

                if !matches!(evt, WatcherEvent::Error(_)) && !self.watcher_rescan_pending {
                    self.watcher_rescan_pending = true;
                    tasks.push(future(async {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        Action::App(ViewerMessage::WatcherRescan)
                    }));
                }
            }
            ViewerMessage::WatcherRescan => {
                self.watcher_rescan_pending = false;
                tasks.push(self.reload_image_list());
            }
            ViewerMessage::SetWallpaper => {
                self.context_menu_position = None;
                if let Some(path) = self.nav.current().cloned() {
                    #[cfg(target_os = "linux")]
                    {
                        if is_cosmic_desktop() {
                            match self.config.wallpaper_behavior {
                                viewer_config::WallpaperBehavior::Ask => {
                                    self.available_outputs = get_cosmic_outputs();
                                    if self.available_outputs.len() <= 1 {
                                        return future(async move {
                                            let result = set_wallpaper_cosmic_on(&path, None);
                                            Action::App(ViewerMessage::WallpaperResult(result))
                                        });
                                    }
                                    self.wallpaper_dialog = Some(path);
                                }
                                viewer_config::WallpaperBehavior::AllDisplays => {
                                    return future(async move {
                                        let result = set_wallpaper_cosmic_on(&path, None);
                                        Action::App(ViewerMessage::WallpaperResult(result))
                                    });
                                }
                                viewer_config::WallpaperBehavior::CurrentDisplay => {
                                    let outputs = get_cosmic_outputs();
                                    let output = outputs.first().cloned();
                                    return future(async move {
                                        let result =
                                            set_wallpaper_cosmic_on(&path, output.as_deref());
                                        Action::App(ViewerMessage::WallpaperResult(result))
                                    });
                                }
                            }
                        } else {
                            // Non-COSMIC Linux: set on all outputs
                            return future(async move {
                                let result = set_wallpaper_cosmic_on(&path, None);
                                Action::App(ViewerMessage::WallpaperResult(result))
                            });
                        }
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        let result = set_wallpaper_portable(&path);
                        return self.update(ViewerMessage::WallpaperResult(result));
                    }
                }
            }
            ViewerMessage::SetWallpaperOn(path, target) => {
                self.wallpaper_dialog = None;
                let output = match target {
                    crate::message::WallpaperTarget::All => None,
                    crate::message::WallpaperTarget::Output(name) => Some(name),
                };
                return future(async move {
                    let result = set_wallpaper_cosmic_on(&path, output.as_deref());
                    Action::App(ViewerMessage::WallpaperResult(result))
                });
            }
            ViewerMessage::CloseWallpaperDialog => {
                self.wallpaper_dialog = None;
            }
            ViewerMessage::WallpaperResult(result) => {
                if let Err(err) = result {
                    tracing::error!("Failed to set wallpaper: {err}");
                }
            }
            ViewerMessage::MoveToTrash => {
                self.context_menu_position = None;
                if let Some(path) = self.nav.current().cloned() {
                    let next_idx = self.nav.index().and_then(|idx| {
                        if idx > 0 {
                            Some(idx - 1)
                        } else if self.nav.total() > 1 {
                            Some(1)
                        } else {
                            None
                        }
                    });
                    if let Some(idx) = next_idx {
                        tasks.push(self.update(ViewerMessage::Nav(NavMessage::GridActivate(idx))));
                    } else {
                        self.viewport.set_image(None, None);
                    }

                    // Trash the file
                    let file_name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("file")
                        .to_string();
                    let paths: Arc<[PathBuf]> = Arc::from(vec![path.clone()]);

                    tasks.push(future(async move {
                        let result = trash::delete(&path)
                            .map_err(|e| format!("Failed to move to trash: {e}"));
                        Action::App(ViewerMessage::TrashResult(result))
                    }));

                    let label = format!("Moved {file_name} to trash");
                    // Auto-dismiss (the Task `push` returns) needs libcosmic's "tokio" feature.
                    let toast = toaster::Toast::new(label)
                        .action(fl!("toast-undo"), move |id| {
                            ViewerMessage::UndoTrash(id, paths.clone())
                        })
                        .duration(toaster::Duration::Long);
                    tasks.push(self.toasts.push(toast).map(Action::App));
                }
            }
            ViewerMessage::UndoTrash(toast_id, recently_trashed) => {
                self.toasts.remove(toast_id);

                tasks.push(future(async move {
                    // Rescan trash to find matching TrashItem entries
                    let mut items_to_restore = Vec::new();

                    if let Ok(trash_items) = trash::os_limited::list() {
                        for path in &*recently_trashed {
                            let target_parent = path.parent().unwrap_or_else(|| Path::new("/"));
                            let target_name =
                                path.file_name().unwrap_or_default().to_string_lossy();

                            for item in &trash_items {
                                if item.original_parent == target_parent
                                    && item.name == target_name.as_ref()
                                {
                                    items_to_restore.push(item.clone());
                                    break;
                                }
                            }
                        }
                    }
                    Action::App(ViewerMessage::UndoTrashStart(items_to_restore))
                }));
            }
            ViewerMessage::UndoTrashStart(items) => {
                if !items.is_empty() {
                    let restore = future(async move {
                        if let Err(e) = trash::os_limited::restore_all(items) {
                            tracing::error!("failed to restore from trash: {e}");
                        }

                        // Trigger a directory rescan
                        Action::App(ViewerMessage::Cancelled)
                    });

                    // Rescan the directory to pickup the restored image
                    tasks.push(restore.chain(self.reload_image_list()));
                }
            }
            ViewerMessage::TrashResult(result) => {
                if let Err(e) = result {
                    tracing::error!("failed to move to trash: {e}");
                }
            }
            ViewerMessage::CloseToast(toast_id) => {
                self.toasts.remove(toast_id);
            }
            ViewerMessage::KeyPressed(key, modifiers, text) => {
                if self.context_menu_position.is_some() && matches!(key, Key::Named(Named::Escape))
                {
                    self.context_menu_position = None;
                    return Task::none();
                }

                // Enter applies crop, Escape cancels
                if matches!(key, Key::Named(Named::Enter))
                    && self.viewport.active_tool() == Some(ToolKind::Crop)
                {
                    return self.update(ViewerMessage::Edit(EditMessage::CropApply));
                }

                if matches!(key, Key::Named(Named::Escape))
                    && self.viewport.active_tool() == Some(ToolKind::Crop)
                {
                    return self.update(ViewerMessage::Edit(EditMessage::CropCancel));
                }

                if self.move_mode && matches!(key, Key::Named(Named::Escape)) {
                    self.move_mode = false;
                    self.move_target = None;
                    self.move_start = None;
                    return Task::none();
                }

                // Picker open: Escape dismisses just the picker, not markup mode.
                if self.color_picker.get_is_active() && matches!(key, Key::Named(Named::Escape)) {
                    return self.update(ViewerMessage::Edit(EditMessage::ColorPicker(
                        ToggleColorPicker,
                    )));
                }

                if matches!(key, Key::Named(Named::Escape))
                    && self.viewport.active_tool() == Some(ToolKind::Annotate)
                    && !self.text_editing
                {
                    self.viewport.apply_tool();
                    self.viewport.set_active_tool(None);
                    self.viewport.set_preview(None);
                    self.viewport.rebuild_display();
                    return Task::none();
                }

                if self.is_fullscreen && matches!(key, Key::Named(Named::Escape)) {
                    return self.update(ViewerMessage::Canvas(CanvasMessage::Fullscreen));
                }

                if matches!(key, Key::Named(Named::Delete)) && self.viewport.active_tool().is_none()
                {
                    return self.update(ViewerMessage::MoveToTrash);
                }

                let is_text_editing = self.text_editing
                    && self
                        .viewport
                        .preview_mut()
                        .and_then(|preview| preview.as_any_mut().downcast_mut::<TextPreview>())
                        .is_some();

                let preview = self
                    .viewport
                    .preview_mut()
                    .and_then(|preview| preview.as_any_mut().downcast_mut::<TextPreview>());

                if is_text_editing {
                    let shift = modifiers.shift();

                    if modifiers.control() {
                        if let Key::Character(c) = &key {
                            match c.as_str() {
                                "b" => {
                                    self.text_bold = !self.text_bold;
                                    if let Some(preview) = self.viewport.preview_mut()
                                        && let Some(t) =
                                            preview.as_any_mut().downcast_mut::<TextPreview>()
                                    {
                                        if t.has_selection() {
                                            let bold = self.text_bold;
                                            t.apply_attr_to_selection(|a| {
                                                a.weight(if bold {
                                                    cosmic_text::Weight::BOLD
                                                } else {
                                                    cosmic_text::Weight::NORMAL
                                                })
                                            });
                                        }
                                        t.bold = self.text_bold;
                                    }
                                    return Task::none();
                                }
                                "i" => {
                                    self.text_italic = !self.text_italic;
                                    if let Some(preview) = self.viewport.preview_mut()
                                        && let Some(t) =
                                            preview.as_any_mut().downcast_mut::<TextPreview>()
                                    {
                                        if t.has_selection() {
                                            let italic = self.text_italic;
                                            t.apply_attr_to_selection(|a| {
                                                a.style(if italic {
                                                    cosmic_text::Style::Italic
                                                } else {
                                                    cosmic_text::Style::Normal
                                                })
                                            });
                                        }
                                        t.italic = self.text_italic;
                                    }
                                    return Task::none();
                                }
                                "u" => {
                                    self.text_underline = !self.text_underline;
                                    if let Some(preview) = self.viewport.preview_mut()
                                        && let Some(t) =
                                            preview.as_any_mut().downcast_mut::<TextPreview>()
                                    {
                                        if t.has_selection() {
                                            let underline = self.text_underline;
                                            t.apply_attr_to_selection(|a| {
                                                a.metadata(usize::from(underline))
                                            });
                                        }
                                        t.underline = self.text_underline;
                                    }
                                    return Task::none();
                                }
                                "a" => {
                                    if let Some(preview) = self.viewport.preview_mut()
                                        && let Some(t) =
                                            preview.as_any_mut().downcast_mut::<TextPreview>()
                                    {
                                        t.select_all();
                                    }
                                    return Task::none();
                                }
                                "c" => {
                                    if let Some(preview) = self.viewport.preview_ref()
                                        && let Some(t) =
                                            preview.as_any().downcast_ref::<TextPreview>()
                                        && let Some(sel) = t.copy_selection()
                                    {
                                        return cosmic::iced::clipboard::write(sel);
                                    }
                                    return Task::none();
                                }
                                "v" => {
                                    return cosmic::iced::clipboard::read().map(|opt| {
                                        cosmic::Action::App(ViewerMessage::TextPaste(
                                            opt.unwrap_or_default(),
                                        ))
                                    });
                                }
                                "x" => {
                                    if let Some(preview) = self.viewport.preview_ref()
                                        && let Some(t) =
                                            preview.as_any().downcast_ref::<TextPreview>()
                                        && let Some(sel) = t.copy_selection()
                                    {
                                        let task = cosmic::iced::clipboard::write(sel);
                                        if let Some(preview) = self.viewport.preview_mut()
                                            && let Some(t) =
                                                preview.as_any_mut().downcast_mut::<TextPreview>()
                                        {
                                            t.delete_selection();
                                        }
                                        return task;
                                    }
                                    return Task::none();
                                }
                                _ => {}
                            }
                        }
                        return Task::none();
                    }

                    match &key {
                        Key::Named(Named::Enter) if shift => {
                            if let Some(preview) = preview {
                                preview.editor_action(cosmic_text::Action::Enter);
                            }
                        }
                        Key::Named(Named::Enter | Named::Escape) => {
                            if self.color_picker.get_is_active() {
                                _ = self.color_picker.update::<ViewerMessage>(
                                    cosmic::widget::color_picker::ColorPickerUpdate::ToggleColorPicker,
                                );
                            }
                            self.viewport.apply_tool();
                            self.text_editing = false;
                            self.text_bold = false;
                            self.text_italic = false;
                            self.text_underline = false;
                            self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            self.viewport.set_preview(Some(Box::new(TextPreview::new(
                                self.annotate_color.0,
                                self.text_font_size,
                                self.text_font_family,
                                false,
                                false,
                                false,
                                self.text_alignment,
                            ))));
                        }
                        Key::Named(Named::Backspace) => {
                            if let Some(preview) = preview {
                                preview.editor_action(cosmic_text::Action::Backspace);
                            }
                        }
                        Key::Named(Named::Delete) => {
                            if let Some(preview) = preview {
                                preview.editor_action(cosmic_text::Action::Delete);
                            }
                        }
                        Key::Named(Named::ArrowLeft) => {
                            if let Some(preview) = preview {
                                preview.motion_with_shift(cosmic_text::Motion::Left, shift);
                            }
                        }
                        Key::Named(Named::ArrowRight) => {
                            if let Some(preview) = preview {
                                preview.motion_with_shift(cosmic_text::Motion::Right, shift);
                            }
                        }
                        Key::Named(Named::ArrowUp) => {
                            if let Some(preview) = preview {
                                preview.motion_with_shift(cosmic_text::Motion::Up, shift);
                            }
                        }
                        Key::Named(Named::ArrowDown) => {
                            if let Some(preview) = preview {
                                preview.motion_with_shift(cosmic_text::Motion::Down, shift);
                            }
                        }
                        Key::Named(Named::Home) => {
                            if let Some(preview) = preview {
                                preview.motion_with_shift(cosmic_text::Motion::Home, shift);
                            }
                        }
                        Key::Named(Named::End) => {
                            if let Some(preview) = preview {
                                preview.motion_with_shift(cosmic_text::Motion::End, shift);
                            }
                        }
                        Key::Character(c) if c.as_str() == " " => {
                            if let Some(preview) = preview {
                                preview.insert_with_attrs(" ");
                            }
                        }
                        _ => {
                            if let Some(txt) = &text
                                && let Some(preview) = preview
                            {
                                for ch in txt.chars() {
                                    let mut buf = [0u8; 4];
                                    preview.insert_with_attrs(ch.encode_utf8(&mut buf));
                                }
                            }
                        }
                    }

                    let is_modifier = matches!(
                        key,
                        Key::Named(
                            Named::Shift
                                | Named::Control
                                | Named::Alt
                                | Named::Super
                                | Named::CapsLock
                        )
                    );
                    if !is_modifier
                        && let Some(preview) = self.viewport.preview_mut()
                        && let Some(t) = preview.as_any_mut().downcast_mut::<TextPreview>()
                    {
                        t.sync_format_at_cursor();
                        self.text_bold = t.bold;
                        self.text_italic = t.italic;
                        self.text_underline = t.underline;
                        self.text_font_size = t.font_size;
                        self.text_font_family = t.font_family;
                        self.text_font_index =
                            self.font_families.iter().position(|&f| f == t.font_family);
                        self.annotate_color = AnnotateColor(t.color);
                    }
                } else if !self.core().nav_bar_active()
                    && matches!(key, Key::Named(Named::ArrowLeft))
                {
                    let idx = self.nav.index().unwrap_or(0);
                    if idx > 0 {
                        return self.update(ViewerMessage::Nav(NavMessage::GridActivate(idx - 1)));
                    }
                } else if !self.core().nav_bar_active()
                    && matches!(key, Key::Named(Named::ArrowRight))
                {
                    let idx = self.nav.index().unwrap_or(0);
                    if idx + 1 < self.nav.total() {
                        return self.update(ViewerMessage::Nav(NavMessage::GridActivate(idx + 1)));
                    }
                } else if let Some(msg) = keyboard_shortcut_handler(key, modifiers, text) {
                    return self.update(msg);
                }
            }
            ViewerMessage::WindowResized(size) => {
                self.window_width = Some(size.width);
                let narrow = self.is_narrow();
                if !self.is_fullscreen && narrow != self.was_narrow {
                    if narrow && self.core().nav_bar_active() {
                        self.core.nav_bar_toggle_condensed();
                    }
                    self.was_narrow = narrow;
                }
            }
            ViewerMessage::Nav(msg) => match msg {
                NavMessage::ScanComplete(dir, images, select) => {
                    self.viewport.cancel_tool();
                    self.viewport.set_image(None, None);

                    self.nav.set_images(dir, images, select.as_deref());

                    if self.nav.index().is_none() && !self.nav.is_empty() {
                        self.nav.select(0);
                    }

                    //self.rebuild_grid_items();
                    self.rebuild_nav_handles();
                    if !self.nav.is_empty() {
                        let idx = self.nav.index().unwrap_or(0);
                        tasks.push(self.load_nearby_thumbnails(idx, 10));
                    }

                    // If no image is selected on load, start loading from image 1 (idx = 0)
                    /*if !self.nav.is_empty() {
                        tasks.push(self.load_remaining_thumbnails());
                    }*/

                    // Load the selected image and the 40 images around it (20 up and 20 down)
                    if let Some(idx) = self.nav.index() {
                        if let Some(path) = self.nav.current().cloned() {
                            tasks.push(self.load_full_image(path));
                        }

                        let offset = self.grid_scroll_offset(idx);
                        tasks.push(scroll_to(
                            self.scroll_id.clone(),
                            AbsoluteOffset {
                                x: Some(0.0),
                                y: Some(offset),
                            },
                        ));
                    }
                }
                NavMessage::NavThumbnailShow(img) => {
                    if self
                        .nav_handles
                        .get(img)
                        .is_some_and(std::option::Option::is_none)
                        && let Some(path) = self.nav.images().get(img).cloned()
                    {
                        if let Some(handle) = self.cache.get_thumbnail(&path) {
                            if let Some(slot) = self.nav_handles.get_mut(img) {
                                *slot = Some(handle);
                            }
                        } else if !self.cache.is_thumbnail_pending(&path) {
                            self.cache.set_thumbnail_pending(path.clone());
                            let max_size = self.config.thumbnail_size.pixels();
                            let cache = self.cache.clone();

                            tasks.push(future(async move {
                                match load_thumbnail(path.clone(), max_size).await {
                                    Ok(loaded) => {
                                        cache.insert_thumbnail(loaded.path.clone(), loaded.handle);
                                        Action::App(ViewerMessage::Image(
                                            ImageMessage::ThumbnailReady(
                                                loaded.path,
                                                loaded.width,
                                                loaded.height,
                                            ),
                                        ))
                                    }
                                    Err(_) => Action::App(ViewerMessage::Image(
                                        ImageMessage::LoadError(path),
                                    )),
                                }
                            }));
                        }
                    }
                }
                NavMessage::NavThumbnailHide(img) => {
                    if let Some(slot) = self.nav_handles.get_mut(img) {
                        *slot = None;
                    }

                    if Some(img) != self.nav.index()
                        && let Some(path) = self.nav.images().get(img)
                    {
                        self.cache.remove_full(path);
                    }
                }
                NavMessage::GridActivate(idx) => {
                    if !self.text_editing
                        && let Some(path) = self.nav.select(idx)
                    {
                        let path = path.clone();
                        // self.grid.set_focused(Some(idx));
                        //self.grid.set_selected(vec![idx]);

                        self.viewport.cancel_tool();
                        self.text_editing = false;
                        if self.color_picker.get_is_active() {
                            _ = self
                                .color_picker
                                .update::<ViewerMessage>(ColorPickerUpdate::ToggleColorPicker);
                        }

                        if let Some(cached) = self.cache.get_full(&path) {
                            self.viewport.revert_all();
                            self.viewport.set_image(
                                Some(CanvasImage {
                                    handle: cached.handle,
                                    width: cached.width,
                                    height: cached.height,
                                }),
                                Some(cached.image),
                            );
                        }
                        // Don't clear the viewport — keep the previous image
                        // visible until the new one loads

                        tasks.push(self.load_full_image(path));
                        let images = self.nav.images().to_vec();
                        for (adj_idx, adj_path) in images.iter().enumerate() {
                            // distance from the just-activated image to this neighbor;
                            // skip the activated image itself, which is loaded above.
                            let dist = idx.abs_diff(adj_idx);
                            if dist != 0
                                && dist <= 1
                                && self.cache.get_full(adj_path).is_none()
                                && !self.cache.is_pending(adj_path)
                            {
                                tasks.push(self.load_full_image(adj_path.clone()));
                            } else if dist > 2 {
                                self.cache.remove_full(adj_path);
                            }
                        }
                    }
                }
                NavMessage::GridFocus(idx) => {
                    self.nav.select(idx);
                }
                NavMessage::GridScroll(offset) => {
                    tasks.push(scroll_to(
                        self.scroll_id.clone(),
                        AbsoluteOffset {
                            x: Some(0.0),
                            y: Some(offset),
                        },
                    ));
                }
                NavMessage::DirectoryRefreshed(images) => {
                    let selected = self.nav.current().cloned();
                    let dir = self.nav.dir().map(std::path::Path::to_path_buf);
                    if let Some(dir) = dir {
                        self.nav.set_images(dir, images, selected.as_deref());
                        self.rebuild_nav_handles();

                        if !self.nav.is_empty() {
                            let idx = self.nav.index().unwrap_or(0);
                            tasks.push(self.load_nearby_thumbnails(idx, 10));
                        }

                        if let Some(path) = self.nav.current().cloned()
                            && self.cache.get_full(&path).is_none()
                        {
                            tasks.push(self.load_full_image(path));
                        }
                    }
                }
            },
            ViewerMessage::Image(msg) => match msg {
                ImageMessage::ThumbnailReady(path, _, _) => {
                    if let Some(handle) = self.cache.get_thumbnail(&path)
                        && let Some(idx) = self.nav.images().iter().position(|pos| pos == &path)
                        && let Some(slot) = self.nav_handles.get_mut(idx)
                    {
                        *slot = Some(handle);
                    }
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
                                self.viewport.set_image(
                                    Some(CanvasImage {
                                        handle: cached.handle,
                                        width: cached.width,
                                        height: cached.height,
                                    }),
                                    Some(cached.image),
                                );
                            }
                        }
                        tasks.push(self.load_nearby_thumbnails(idx, 5));
                    }
                }
                ImageMessage::LoadError(path) => {
                    self.cache.clear_pending(&path);
                    self.cache.clear_pending_thumbnail(&path);
                }
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
                CanvasMessage::ContextMenu(point) => {
                    self.context_menu_position = point;
                }
                CanvasMessage::ZoomIn => {
                    let bounds = self.viewport.last_bounds().get();
                    let viewport = Size::new(bounds.width, bounds.height);
                    let before = self.viewport.actual_percent(viewport);
                    let old_zoom = self.viewport.zoom();
                    let new_zoom = (old_zoom * 1.25).min(10.0);

                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_zoom_changed(old_zoom, new_zoom, size);
                    }

                    self.viewport.set_zoom(new_zoom);
                    if before < 99.9 && self.viewport.actual_percent(viewport) > 100.0 {
                        self.viewport.set_actual_percent(100.0, viewport);
                    }

                    // Cap zoom in to 500%
                    if self.viewport.actual_percent(viewport) > 500.0 {
                        self.viewport.set_actual_percent(500.0, viewport);
                    }
                }
                CanvasMessage::ZoomOut => {
                    let bounds = self.viewport.last_bounds().get();
                    let viewport = Size::new(bounds.width, bounds.height);
                    let before = self.viewport.actual_percent(viewport);
                    let old_zoom = self.viewport.zoom();
                    let floor = if self.viewport.active_tool() == Some(ToolKind::Crop) {
                        // Crop floor = fit-to-screen, so the whole image stays visible.
                        1.0
                    } else {
                        0.1
                    };
                    let new_zoom = (old_zoom / 1.25).max(floor);
                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_zoom_changed(old_zoom, new_zoom, size);
                    }

                    self.viewport.set_zoom(new_zoom);
                    if before > 100.1 && self.viewport.actual_percent(viewport) < 100.0 {
                        self.viewport.set_actual_percent(100.0, viewport);
                    }
                }
                CanvasMessage::Pan(pan) => {
                    if self.viewport.active_tool() == Some(ToolKind::Crop) {
                        let bounds = self.viewport.last_bounds().get();
                        let crop_region = self
                            .viewport
                            .preview_ref()
                            .and_then(|p| p.as_any().downcast_ref::<CropSelection>())
                            .map(|c| c.region);
                        if let Some(size) = self.viewport.image_size() {
                            let fit_scale =
                                (bounds.width / size.width).min(bounds.height / size.height);
                            let zoom = self.viewport.zoom();
                            // Pan limit = how far the zoomed image extends past the static crop frame.
                            let (rw, rh) = crop_region
                                .map_or((size.width, size.height), |r| (r.width, r.height));
                            let max_x = fit_scale * size.width.mul_add(zoom, -rw).max(0.0) / 2.0;
                            let max_y = fit_scale * size.height.mul_add(zoom, -rh).max(0.0) / 2.0;
                            self.viewport.set_pan(Vector::new(
                                pan.x.clamp(-max_x, max_x),
                                pan.y.clamp(-max_y, max_y),
                            ));
                        }
                    } else {
                        self.viewport.set_pan(pan);
                    }
                }
                CanvasMessage::FitToView => {
                    if self.viewport.active_tool() != Some(ToolKind::Crop) {
                        self.viewport.set_zoom(1.0);
                        self.viewport.set_pan(Vector::ZERO);
                    }
                }
                CanvasMessage::ActualSize => {
                    let bounds = self.viewport.last_bounds().get();
                    let viewport_size = Size::new(bounds.width, bounds.height);
                    self.viewport.zoom_to_actual_size(viewport_size);
                }
                CanvasMessage::Fullscreen => {
                    self.is_fullscreen = !self.is_fullscreen;

                    if self.is_fullscreen {
                        self.saved_nav_toggled = self.core.nav_bar_active();
                        self.core.window.show_headerbar = false;
                        self.core.nav_bar_set_toggled(false);
                    } else {
                        self.core.window.show_headerbar = true;
                        self.core.nav_bar_set_toggled(self.saved_nav_toggled);
                    }

                    if let Some(window_id) = self.core.main_window_id() {
                        return window::set_mode(
                            window_id,
                            if self.is_fullscreen {
                                window::Mode::Fullscreen
                            } else {
                                window::Mode::Windowed
                            },
                        );
                    }
                }
                CanvasMessage::ToolStart(point) => {
                    if self.context_menu_position.is_some() {
                        self.context_menu_position = None;
                        return Task::none();
                    }

                    if self.move_mode {
                        let hit = self
                            .viewport
                            .operations_mut()
                            .iter()
                            .rposition(|op| op.movable() && op.hit_test(point));

                        self.move_target = hit;
                        self.move_start = Some(point);
                        self.viewport.tool_dragging = true;
                        return Task::none();
                    }

                    if self.viewport.active_tool() == Some(ToolKind::Annotate)
                        && matches!(self.annotate_tool, AnnotateTool::Text)
                    {
                        if self.text_editing {
                            let on_box = self
                                .viewport
                                .preview_ref()
                                .and_then(|p| p.as_any().downcast_ref::<TextPreview>())
                                .is_some_and(|t| t.handle_at(point) != TextDragHandle::None);

                            if on_box {
                                if let Some(size) = self.viewport.image_size()
                                    && let Some(preview) = self.viewport.preview_mut()
                                {
                                    preview.on_press(point, size);
                                    self.viewport.tool_dragging = true;
                                }
                                if let Some(preview) = self.viewport.preview_mut()
                                    && let Some(t) =
                                        preview.as_any_mut().downcast_mut::<TextPreview>()
                                {
                                    t.sync_format_at_cursor();
                                    self.text_bold = t.bold;
                                    self.text_italic = t.italic;
                                    self.text_underline = t.underline;
                                }
                                return Task::none();
                            }

                            if self.color_picker.get_is_active() {
                                _ = self.color_picker.update::<ViewerMessage>(
                                    cosmic::widget::color_picker::ColorPickerUpdate::ToggleColorPicker,
                                );
                            }
                            self.viewport.apply_tool();
                            self.text_editing = false;
                            self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            self.viewport.set_preview(Some(Box::new(TextPreview::new(
                                self.annotate_color.0,
                                self.text_font_size,
                                self.text_font_family,
                                self.text_bold,
                                self.text_italic,
                                self.text_underline,
                                self.text_alignment,
                            ))));
                            self.viewport.rebuild_display();
                            return Task::none();
                        }

                        let re_edit = {
                            let ops = self.viewport.operations_mut();
                            let hit = ops.iter().rposition(|op| {
                                op.as_any()
                                    .downcast_ref::<TextOperation>()
                                    .is_some_and(|t| t.hit_test(point))
                            });

                            hit.and_then(|idx| {
                                let op = ops.remove(idx);
                                op.as_any()
                                    .downcast_ref::<TextOperation>()
                                    .map(viewer_tools::annotate::TextOperation::to_preview)
                            })
                        };

                        if let Some(preview) = re_edit {
                            self.text_bold = preview.bold;
                            self.text_italic = preview.italic;
                            self.text_underline = preview.underline;
                            self.text_font_size = preview.font_size;
                            self.text_font_family = preview.font_family;
                            self.text_alignment = preview.alignment;
                            self.annotate_color = AnnotateColor(preview.color);
                            self.text_editing = true;
                            self.viewport.set_preview(Some(Box::new(preview)));
                            self.viewport.rebuild_display();
                            return Task::none();
                        }

                        self.viewport.set_preview(Some(Box::new(TextPreview::new(
                            self.annotate_color.0,
                            self.text_font_size,
                            self.text_font_family,
                            self.text_bold,
                            self.text_italic,
                            self.text_underline,
                            self.text_alignment,
                        ))));
                    }

                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_press(point, size);
                        self.viewport.tool_dragging = true;
                    }
                }
                CanvasMessage::ToolDrag(point) => {
                    if self.move_mode {
                        if let Some(idx) = self.move_target
                            && let Some(start) = self.move_start
                        {
                            let dx = point.x - start.x;
                            let dy = point.y - start.y;
                            if let Some(op) = self.viewport.operations_mut().get_mut(idx) {
                                op.translate(dx, dy);
                            }
                            self.move_start = Some(point);
                        }
                    } else if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_drag(point, size);
                    }
                }
                CanvasMessage::ToolEnd => {
                    if self.move_mode {
                        self.move_target = None;
                        self.move_start = None;
                        self.viewport.tool_dragging = false;
                        return Task::none();
                    }
                    self.viewport.tool_dragging = false;
                    if let Some(size) = self.viewport.image_size()
                        && let Some(preview) = self.viewport.preview_mut()
                    {
                        preview.on_release(Point::ORIGIN, size);
                    }

                    // Crop is applied only via Apply/Enter, never on drag-release, so the frame
                    // stays live for further adjust/pan and behaves the same in Custom and ratio modes.

                    // Set text_editing flag when text preview enters editing mode
                    if self.viewport.active_tool() == Some(ToolKind::Annotate)
                        && matches!(self.annotate_tool, AnnotateTool::Text)
                        && let Some(preview) = self.viewport.preview_mut()
                        && let Some(text_preview) =
                            preview.as_any_mut().downcast_mut::<TextPreview>()
                    {
                        self.text_editing = text_preview.is_editing();
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
                                self.highlighter_stroke_size,
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
                if !matches!(msg, EditMessage::ColorPicker(_)) && self.color_picker.get_is_active()
                {
                    if let Some(color) = self.color_picker.get_applied_color() {
                        self.annotate_color = AnnotateColor(color);
                        self.save_last_color();
                    }
                    _ = self.color_picker.update::<ViewerMessage>(
                        cosmic::widget::color_picker::ColorPickerUpdate::ToggleColorPicker,
                    );
                }
                match msg {
                    EditMessage::Annotate => {
                        if self.viewport.active_tool() != Some(ToolKind::Annotate) {
                            self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            self.viewport.set_preview(Some(Box::new(PenPreview::new(
                                self.annotate_color.0,
                                self.annotate_stroke_size,
                            ))));
                        }
                    }
                    EditMessage::AnnotateApply => {
                        self.viewport.apply_tool();
                        self.viewport.set_active_tool(None);
                        self.viewport.set_preview(None);
                        self.text_editing = false;
                        self.show_text_format_menu = false;
                        self.viewport.tool_dragging = false;
                    }
                    EditMessage::AnnotateCancel => {
                        if self.text_editing {
                            if self.color_picker.get_is_active() {
                                _ = self
                                    .color_picker
                                    .update::<ViewerMessage>(ColorPickerUpdate::ToggleColorPicker);
                            }
                            self.text_editing = false;
                        }
                        self.viewport.cancel_tool();
                        self.viewport.revert_all();
                        self.viewport.set_active_tool(None);
                        self.viewport.set_preview(None);
                        // Restore working image from cache so rotation etc. still works
                        if let Some(path) = self.nav.current().cloned()
                            && let Some(cached) = self.cache.get_full(&path)
                        {
                            self.viewport.set_image(
                                Some(CanvasImage {
                                    handle: cached.handle,
                                    width: cached.width,
                                    height: cached.height,
                                }),
                                Some(cached.image),
                            );
                        }
                    }
                    EditMessage::AnnotateStroke(size) => {
                        if self.annotate_tool == AnnotateTool::Highlighter {
                            let sizes: [f32; 7] = [8., 10., 12., 14., 16., 18., 20.];
                            if let Some(&size) = sizes.get(size) {
                                self.highlighter_stroke_size = size;
                                if let Some(highlighter) =
                                    self.viewport.preview_mut().and_then(|preview| {
                                        preview.as_any_mut().downcast_mut::<HighlighterPreview>()
                                    })
                                {
                                    highlighter.width = size;
                                }
                            }
                        } else {
                            let sizes: [f32; 6] = [2., 4., 6., 8., 10., 12.];
                            if let Some(&size) = sizes.get(size) {
                                self.annotate_stroke_size = size;
                                if let Some(preview) = self.viewport.preview_mut() {
                                    if let Some(pen) =
                                        preview.as_any_mut().downcast_mut::<PenPreview>()
                                    {
                                        pen.width = size;
                                    } else if let Some(pencil) =
                                        preview.as_any_mut().downcast_mut::<PencilPreview>()
                                    {
                                        pencil.width = size;
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
                    }
                    EditMessage::AnnotateTool(tool) => {
                        self.annotate_tool = tool;
                        self.show_text_format_menu = false;
                        match tool {
                            AnnotateTool::Highlighter => {
                                self.viewport
                                    .set_preview(Some(Box::new(HighlighterPreview::new(
                                        self.annotate_color.0,
                                        self.highlighter_stroke_size,
                                    ))));
                            }
                            AnnotateTool::Pen => {
                                self.viewport.set_preview(Some(Box::new(PenPreview::new(
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
                                if self.shape_popup {
                                    self.shape_popup = false;
                                }
                                let kind = match tool {
                                    AnnotateTool::Rectangle => ShapeKind::Rectangle,
                                    AnnotateTool::Ellipse => ShapeKind::Ellipse,
                                    AnnotateTool::Line => ShapeKind::Line,
                                    AnnotateTool::Arrow => ShapeKind::Arrow,
                                    AnnotateTool::Star => ShapeKind::Star,
                                    AnnotateTool::Polygon => ShapeKind::Polygon,
                                    _ => unreachable!(),
                                };
                                self.selected_shape = tool;
                                self.viewport.set_preview(Some(Box::new(ShapePreview::new(
                                    kind,
                                    self.annotate_color.0,
                                    self.annotate_stroke_size,
                                ))));
                            }
                            AnnotateTool::Text => {
                                self.viewport.set_preview(Some(Box::new(TextPreview::new(
                                    self.annotate_color.0,
                                    self.text_font_size,
                                    self.text_font_family,
                                    self.text_bold,
                                    self.text_italic,
                                    self.text_underline,
                                    self.text_alignment,
                                ))));
                                self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            }
                        }
                    }
                    EditMessage::AnnotateColor(color) => {
                        self.annotate_color = color;
                        self.save_last_color();

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
                                if text.has_selection() {
                                    let c = color.0;
                                    text.apply_attr_to_selection(|a| a.color(text_color_rgba(c)));
                                }
                            }
                        }
                        self.viewport.rebuild_display();
                    }
                    EditMessage::Crop => {
                        if self.viewport.active_tool() != Some(ToolKind::Crop) {
                            if self.viewport.zoom() < 1.0 {
                                self.viewport.set_zoom(1.0);
                                self.viewport.set_pan(Vector::ZERO);
                            }
                            self.viewport.set_active_tool(Some(ToolKind::Crop));
                            let mut selection = CropSelection::new();
                            if let Some(img_size) = self.viewport.image_size() {
                                selection.activate(CropRatio::Custom, img_size);
                                self.crop_ratio = CropRatio::Custom;
                            }
                            self.viewport.set_preview(Some(Box::new(selection)));
                        }
                    }
                    EditMessage::CropApply => {
                        let fit_region = self
                            .viewport
                            .preview_mut()
                            .and_then(|preview| {
                                preview.as_any_mut().downcast_mut::<CropSelection>()
                            })
                            .map(|sel| sel.region);

                        if let Some(fit_region) = fit_region
                            && let Some(size) = self.viewport.image_size()
                        {
                            let zoom = self.viewport.zoom();
                            let pan = self.viewport.pan();
                            let bounds = self.viewport.last_bounds().get();
                            let fit_scale =
                                (bounds.width / size.width).min(bounds.height / size.height);

                            // default (unzoomed, un-panned) view: zoom is set to exactly 1.0 on reset.
                            let region = if (zoom - 1.0).abs() < f32::EPSILON && pan == Vector::ZERO
                            {
                                fit_region
                            } else {
                                let cx = size.width / 2.0;
                                let cy = size.height / 2.0;
                                Rectangle::new(
                                    Point::new(
                                        fit_region.x / zoom + cx * (1.0 - 1.0 / zoom)
                                            - pan.x / (zoom * fit_scale),
                                        fit_region.y / zoom + cy * (1.0 - 1.0 / zoom)
                                            - pan.y / (zoom * fit_scale),
                                    ),
                                    Size::new(fit_region.width / zoom, fit_region.height / zoom),
                                )
                            };

                            // crop_to_region casts to u32 unclamped; clamp the off-image case.
                            let x = region.x.clamp(0.0, size.width);
                            let y = region.y.clamp(0.0, size.height);
                            let w = region.width.min(size.width - x).max(1.0);
                            let h = region.height.min(size.height - y).max(1.0);
                            let region = Rectangle::new(Point::new(x, y), Size::new(w, h));

                            for op in self.viewport.operations_mut() {
                                op.transform_crop(region);
                            }

                            self.viewport.apply_tool();

                            if let Some(img) = self.viewport.working_image_mut() {
                                *img = crop_to_region(img, region);
                            }
                            self.viewport.rebuild_display();
                        }
                    }
                    EditMessage::CropCancel => {
                        self.viewport.cancel_tool();
                        if let Some(current) = self.nav.current() {
                            self.viewport.revert_all();
                            self.cache.remove_full(current);
                            tasks.push(self.load_full_image(current.clone()));
                        }
                    }
                    EditMessage::CropRatioPopupToggle => {
                        self.crop_ratio_popup = !self.crop_ratio_popup;
                    }
                    EditMessage::CropRatio(ratio) => {
                        let img_size = self.viewport.image_size();
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(crop) = preview.as_any_mut().downcast_mut::<CropSelection>()
                            && let Some(size) = img_size
                        {
                            crop.set_ratio(ratio, size);
                            self.crop_ratio = ratio;
                        }
                    }
                    EditMessage::RotateLeft => {
                        let is_cropping = self.viewport.active_tool() == Some(ToolKind::Crop);
                        // Commit an in-progress annotation (e.g. a text box being edited) so it
                        // rotates with the image instead of being discarded; `apply_tool` is a
                        // no-op (returns false) for an empty preview, which then cancels.
                        if !is_cropping && !self.viewport.apply_tool() {
                            self.viewport.cancel_tool();
                        }
                        let direction = RotateDirection::Left;
                        let image_size = self.viewport.image_size().unwrap_or(Size::ZERO);

                        for op in self.viewport.operations_mut() {
                            op.transform_rotate(direction, image_size);
                        }

                        self.viewport
                            .commit(Box::new(RotateOperation::new(direction)));

                        if let Some(img) = self.viewport.working_image_mut() {
                            *img = img.rotate270();
                        }
                        self.viewport.rebuild_display_preserve_view();

                        if is_cropping {
                            let new_size = self.viewport.image_size().unwrap_or(Size::ZERO);
                            if let Some(preview) = self.viewport.preview_mut()
                                && let Some(crop) =
                                    preview.as_any_mut().downcast_mut::<CropSelection>()
                            {
                                crop.activate(self.crop_ratio, new_size);
                            }
                        }
                    }
                    EditMessage::RotateRight => {
                        let is_cropping = self.viewport.active_tool() == Some(ToolKind::Crop);
                        // Commit an in-progress annotation (e.g. a text box being edited) so it
                        // rotates with the image instead of being discarded; `apply_tool` is a
                        // no-op (returns false) for an empty preview, which then cancels.
                        if !is_cropping && !self.viewport.apply_tool() {
                            self.viewport.cancel_tool();
                        }
                        let direction = RotateDirection::Right;
                        let image_size = self.viewport.image_size().unwrap_or(Size::ZERO);

                        for op in self.viewport.operations_mut() {
                            op.transform_rotate(direction, image_size);
                        }

                        self.viewport
                            .commit(Box::new(RotateOperation::new(direction)));

                        if let Some(img) = self.viewport.working_image_mut() {
                            *img = img.rotate90();
                        }
                        self.viewport.rebuild_display_preserve_view();

                        if is_cropping {
                            let new_size = self.viewport.image_size().unwrap_or(Size::ZERO);
                            if let Some(preview) = self.viewport.preview_mut()
                                && let Some(crop) =
                                    preview.as_any_mut().downcast_mut::<CropSelection>()
                            {
                                crop.activate(self.crop_ratio, new_size);
                            }
                        }
                    }
                    EditMessage::ShapePopupOpen => self.shape_popup = true,
                    EditMessage::ShapePopupClose => self.shape_popup = false,
                    EditMessage::StrokePopupToggle => self.stroke_popup = !self.stroke_popup,
                    EditMessage::ColorPicker(update) => {
                        let was_active = self.color_picker.get_is_active();

                        match &update {
                            AppliedColor => {
                                tasks.push(
                                    self.color_picker
                                        .update::<ViewerMessage>(update)
                                        .map(|_| Action::None),
                                );
                                if let Some(color) = self.color_picker.get_applied_color() {
                                    self.annotate_color = AnnotateColor(color);
                                    self.save_last_color();
                                }
                                if self.color_picker.get_is_active() {
                                    _ = self
                                        .color_picker
                                        .update::<ViewerMessage>(ToggleColorPicker);
                                }
                            }
                            Cancel => {
                                tasks.push(
                                    self.color_picker
                                        .update::<ViewerMessage>(update)
                                        .map(|_| Action::None),
                                );
                                if self.color_picker.get_is_active() {
                                    _ = self
                                        .color_picker
                                        .update::<ViewerMessage>(ToggleColorPicker);
                                }
                            }
                            ToggleColorPicker => {
                                // Closing via toggle — apply the color
                                if was_active
                                    && let Some(color) = self.color_picker.get_applied_color()
                                {
                                    self.annotate_color = AnnotateColor(color);
                                    self.save_last_color();
                                }
                                tasks.push(
                                    self.color_picker
                                        .update::<ViewerMessage>(update)
                                        .map(|_| Action::None),
                                );
                            }
                            ColorPickerUpdate::ActionFinished => {
                                if let Some(color) = self.color_picker.get_applied_color() {
                                    self.annotate_color = AnnotateColor(color);
                                    self.has_custom_color = true;
                                }

                                tasks.push(
                                    self.color_picker
                                        .update::<ViewerMessage>(update)
                                        .map(|_| Action::None),
                                );

                                // ActionFinished auto-closes the picker in the model;
                                // reopen it so the user can keep adjusting until they
                                // are explicitly done.
                                if !self.color_picker.get_is_active() {
                                    _ = self
                                        .color_picker
                                        .update::<ViewerMessage>(ToggleColorPicker);
                                }
                            }
                            _ => {
                                tasks.push(
                                    self.color_picker
                                        .update::<ViewerMessage>(update)
                                        .map(|_| Action::None),
                                );
                            }
                        }

                        if let Some(preview) = self.viewport.preview_mut() {
                            if let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>() {
                                text.color = self.annotate_color.0;
                                if text.has_selection() {
                                    let c = self.annotate_color.0;
                                    text.apply_attr_to_selection(|a| a.color(text_color_rgba(c)));
                                }
                            } else if let Some(pen) =
                                preview.as_any_mut().downcast_mut::<PenPreview>()
                            {
                                pen.color = self.annotate_color.0;
                            } else if let Some(pencil) =
                                preview.as_any_mut().downcast_mut::<PencilPreview>()
                            {
                                pencil.color = self.annotate_color.0;
                            } else if let Some(highlighter) =
                                preview.as_any_mut().downcast_mut::<HighlighterPreview>()
                            {
                                highlighter.color = self.annotate_color.0;
                            } else if let Some(shape) =
                                preview.as_any_mut().downcast_mut::<ShapePreview>()
                            {
                                shape.color = self.annotate_color.0;
                            }
                        }
                        self.viewport.rebuild_display();
                    }
                    EditMessage::ToggleTextFormatMenu => {
                        self.show_text_format_menu = !self.show_text_format_menu;
                    }
                    EditMessage::TextBold => {
                        self.text_bold = !self.text_bold;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            if text.has_selection() {
                                let bold = self.text_bold;
                                text.apply_attr_to_selection(|a| {
                                    a.weight(if bold {
                                        cosmic_text::Weight::BOLD
                                    } else {
                                        cosmic_text::Weight::NORMAL
                                    })
                                });
                            }
                            text.bold = self.text_bold;
                        }
                        self.viewport.rebuild_display();
                    }
                    EditMessage::TextItalic => {
                        self.text_italic = !self.text_italic;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            if text.has_selection() {
                                let italic = self.text_italic;
                                text.apply_attr_to_selection(|a| {
                                    a.style(if italic {
                                        cosmic_text::Style::Italic
                                    } else {
                                        cosmic_text::Style::Normal
                                    })
                                });
                            }
                            text.italic = self.text_italic;
                        }
                        self.viewport.rebuild_display();
                    }
                    EditMessage::TextUnderline => {
                        self.text_underline = !self.text_underline;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            if text.has_selection() {
                                let underline = self.text_underline;
                                text.apply_attr_to_selection(|a| {
                                    a.metadata(usize::from(underline))
                                });
                            }
                            text.underline = self.text_underline;
                        }
                        self.viewport.rebuild_display();
                    }
                    EditMessage::TextAlignment(alignment) => {
                        self.text_alignment = alignment;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            text.set_line_alignment(alignment);
                        }
                        self.viewport.rebuild_display();
                    }
                    EditMessage::TextFontSize(idx) => {
                        let sizes = [12.0_f32, 16.0, 20.0, 24.0, 32.0, 48.0, 64.0];
                        if let Some(&size) = sizes.get(idx) {
                            self.text_font_size = size;
                            if let Some(preview) = self.viewport.preview_mut()
                                && let Some(text) =
                                    preview.as_any_mut().downcast_mut::<TextPreview>()
                            {
                                text.update_font_size(size);
                            }
                            self.viewport.rebuild_display();
                        }
                    }
                    EditMessage::TextFontFamily(idx) => {
                        self.text_font_index = Some(idx);
                        let fam = self.font_families[idx];
                        self.text_font_family = fam;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            if text.has_selection() {
                                text.apply_attr_to_selection(|a| {
                                    a.family(cosmic_text::Family::Name(fam))
                                });
                            }
                            text.font_family = fam;
                        }
                        self.viewport.rebuild_display();
                    }
                    EditMessage::TextCancel => {
                        self.viewport.cancel_tool();
                        self.text_editing = false;
                    }
                    EditMessage::TextApply => {
                        self.viewport.apply_tool();
                        self.text_editing = false;
                    }
                    EditMessage::ToggleMoveMode => {
                        self.move_mode = !self.move_mode;
                        self.move_target = None;
                        self.move_start = None;
                    }
                    EditMessage::Undo => {
                        if let Some(op) = self.viewport.undo() {
                            if let Some(rotate) = op.as_any().downcast_ref::<RotateOperation>() {
                                let inverse = rotate.direction.inverse();
                                let image_size = self.viewport.image_size().unwrap_or(Size::ZERO);

                                for op in self.viewport.operations_mut() {
                                    op.transform_rotate(inverse, image_size);
                                }

                                if let Some(img) = self.viewport.working_image_mut() {
                                    *img = match inverse {
                                        RotateDirection::Left => img.rotate270(),
                                        RotateDirection::Right => img.rotate90(),
                                    };
                                }
                                self.rebuild_working_image();
                            } else if let Some(crop) = op.as_any().downcast_ref::<CropOperation>() {
                                let region = crop.region;

                                for op in self.viewport.operations_mut() {
                                    op.transform_crop(Rectangle::new(
                                        Point::new(-region.x, -region.y),
                                        Size::ZERO,
                                    ));
                                }

                                self.rebuild_working_image();

                                // Reset the crop preview to the restored image size
                                if let Some(new_size) = self.viewport.image_size()
                                    && let Some(preview) = self.viewport.preview_mut()
                                    && let Some(selection) =
                                        preview.as_any_mut().downcast_mut::<CropSelection>()
                                {
                                    let ratio = selection.ratio;
                                    selection.activate(ratio, new_size);
                                }
                            } else {
                                self.viewport.rebuild_display();
                            }
                        }
                    }
                    EditMessage::Redo => {
                        if let Some(op) = self.viewport.redo() {
                            if let Some(rotate) = op.as_any().downcast_ref::<RotateOperation>() {
                                let direction = rotate.direction;
                                let image_size = self.viewport.image_size().unwrap_or(Size::ZERO);

                                for op in self.viewport.operations_mut() {
                                    op.transform_rotate(direction, image_size);
                                }

                                self.rebuild_working_image();
                            } else if let Some(crop) = op.as_any().downcast_ref::<CropOperation>() {
                                let region = crop.region;

                                for op in self.viewport.operations_mut() {
                                    op.transform_crop(region);
                                }

                                self.rebuild_working_image();
                            } else {
                                self.viewport.rebuild_display();
                            }
                        }
                    }
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
        let watcher_sub =
            crate::watcher::watch_directory(self.nav.dir().map(std::path::Path::to_path_buf))
                .map(ViewerMessage::WatcherEvent);

        Subscription::batch([
            event::listen_with(|event, _status, _id| match event {
                iced::Event::Window(iced::window::Event::Resized(size)) => {
                    Some(ViewerMessage::WindowResized(size))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    text,
                    ..
                }) => Some(ViewerMessage::KeyPressed(key, modifiers, text)),
                _ => None,
            }),
            watcher_sub,
        ])
    }
}

// HELPER FUNCTIONS

fn detail_row<'a>(label: String, value: String) -> Element<'a, ViewerMessage> {
    Column::new()
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

// reason: file sizes never approach 2^52 bytes, so f64 represents them exactly.
// reason: crop region is a non-negative, in-bounds pixel rectangle.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn crop_to_region(img: &DynamicImage, region: Rectangle) -> DynamicImage {
    img.crop_imm(
        region.x as u32,
        region.y as u32,
        region.width as u32,
        region.height as u32,
    )
}

// reason: normalized color channels are in 0.0..=1.0, so *255 lands in u8 range.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn text_color_rgba(c: Color) -> cosmic_text::Color {
    cosmic_text::Color::rgba(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

#[allow(clippy::cast_precision_loss)]
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

// reason: the write guard must live across the whole `.faces()` walk (its single
// use); clippy's auto-merge suggestion can't see through the borrow chain.
#[allow(clippy::significant_drop_tightening)]
fn load_font_families() -> Vec<&'static str> {
    // Scope the write guard so the font-system lock is released before sorting.
    let mut families: Vec<String> = {
        let mut font_sys = font_system().write().expect("Read font system");
        let unique: HashSet<String> = font_sys
            .raw()
            .db()
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .collect();
        unique.into_iter().collect()
    };

    families.sort();

    families
        .into_iter()
        .map(|name| &*Box::leak(name.into_boxed_str()))
        .collect()
}

// Wallpaper functions

fn is_cosmic_desktop() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_uppercase().contains("COSMIC"))
}

fn get_cosmic_outputs() -> Vec<String> {
    if let Ok(output) = std::process::Command::new("cosmic-randr")
        .arg("list")
        .output()
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stripped = strip_ansi_codes(&stdout);
        stripped
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.contains("(enabled)") || line.contains("(disabled)") {
                    line.split_whitespace().next().map(String::from)
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    }
}

fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(not(target_os = "linux"))]
fn set_wallpaper_portable(path: &std::path::Path) -> Result<(), String> {
    wallpaper::set_from_path(
        path.to_str()
            .ok_or_else(|| "Invalid file path".to_string())?,
    )
    .map_err(|e| format!("Failed to set wallpaper: {e}"))
}

fn set_wallpaper_cosmic_on(path: &std::path::Path, output: Option<&str>) -> Result<(), String> {
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?
        .join("cosmic/com.system76.CosmicBackground/v1");

    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config directory: {e}"))?;

    let path_str = path.to_string_lossy();

    let (config_filename, output_field) = output.map_or_else(
        || ("all".to_string(), "all".to_string()),
        |name| (format!("output.{name}"), name.to_string()),
    );

    let config_path = config_dir.join(&config_filename);

    let same_on_all_path = config_dir.join("same-on-all");
    if let Some(output_name) = output {
        std::fs::write(&same_on_all_path, "false\n")
            .map_err(|e| format!("Failed to write same-on-all: {e}"))?;

        update_backgrounds_list(&config_dir, output_name)?;
    } else {
        std::fs::write(&same_on_all_path, "true\n")
            .map_err(|e| format!("Failed to write same-on-all: {e}"))?;
    }

    let content = if config_path.exists() {
        let existing = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {e}"))?;
        update_source_in_config(&existing, &path_str)
    } else {
        format!(
            r#"(
    output: "{output_field}",
    source: Path("{path_str}"),
    filter_by_theme: false,
    rotation_frequency: 300,
    filter_method: Lanczos,
    scaling_mode: Zoom,
    sampling_method: Alphanumeric,
)
"#
        )
    };

    std::fs::write(&config_path, content)
        .map_err(|e| format!("Failed to write config file: {e}"))?;

    add_to_cosmic_settings_custom_images(path)?;

    Ok(())
}

fn add_to_cosmic_settings_custom_images(path: &std::path::Path) -> Result<(), String> {
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?
        .join("cosmic/com.system76.CosmicSettings.Wallpaper/v1");

    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config directory: {e}"))?;

    let custom_images_path = config_dir.join("custom-images");

    let mut custom_images: Vec<PathBuf> = std::fs::read_to_string(&custom_images_path)
        .ok()
        .and_then(|content| parse_path_list(&content))
        .unwrap_or_default();

    let path_buf = path.to_path_buf();
    if !custom_images.contains(&path_buf) {
        custom_images.push(path_buf);
    }

    // Remove any duplicates
    let mut seen = std::collections::HashSet::new();
    custom_images.retain(|p| seen.insert(p.clone()));

    let content = format!(
        "[\n    {},\n]",
        custom_images
            .iter()
            .map(|p| format!("\"{}\"", p.display()))
            .collect::<Vec<_>>()
            .join(",\n    ")
    );
    std::fs::write(&custom_images_path, content)
        .map_err(|e| format!("Failed to write custom-images: {e}"))?;

    Ok(())
}

fn parse_path_list(content: &str) -> Option<Vec<PathBuf>> {
    let trimmed = content.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        Some(
            inner
                .split(',')
                .filter_map(|s| {
                    let s = s.trim().trim_matches('"');
                    if s.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(s))
                    }
                })
                .collect(),
        )
    } else {
        None
    }
}

fn update_backgrounds_list(config_dir: &std::path::Path, output_name: &str) -> Result<(), String> {
    let backgrounds_path = config_dir.join("backgrounds");
    let mut backgrounds: Vec<String> = std::fs::read_to_string(&backgrounds_path)
        .ok()
        .and_then(|content| parse_backgrounds_list(&content))
        .unwrap_or_default();

    if !backgrounds.contains(&output_name.to_string()) {
        backgrounds.push(output_name.to_string());
        let content = format!(
            "[\n    {},\n]",
            backgrounds
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(",\n    ")
        );
        std::fs::write(&backgrounds_path, content)
            .map_err(|e| format!("Failed to write backgrounds: {e}"))?;
    }

    Ok(())
}

fn parse_backgrounds_list(content: &str) -> Option<Vec<String>> {
    let trimmed = content.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        Some(
            inner
                .split(',')
                .filter_map(|s| {
                    let s = s.trim().trim_matches('"');
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                })
                .collect(),
        )
    } else {
        None
    }
}

fn update_source_in_config(existing: &str, new_path: &str) -> String {
    let mut result = String::new();
    let mut skip_until_comma_or_paren = false;

    for line in existing.lines() {
        let trimmed = line.trim();

        if skip_until_comma_or_paren {
            if trimmed.ends_with(',') || trimmed.ends_with(')') {
                skip_until_comma_or_paren = false;
            }
            continue;
        }

        if trimmed.starts_with("source:") {
            writeln!(result, "    source: Path(\"{new_path}\"),")
                .expect("writing to a String is infallible");
            if !trimmed.ends_with(',') && !trimmed.ends_with(')') {
                skip_until_comma_or_paren = true;
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    if !existing.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Override the base Icon appearance with the COSMIC selected-icon-button look when selected:
/// `@selected_color` background + `@accent_text_color` glyph/label. Non-selected = untouched Icon style.
fn apply_selected(mut base: button::Style, selected: bool, t: &Theme) -> button::Style {
    if selected {
        let cosmic = t.cosmic();
        base.background = Some(Background::Color(
            cosmic.icon_button.selected_state_color().into(),
        ));
        base.icon_color = Some(cosmic.accent_text_color().into());
        base.text_color = Some(cosmic.accent_text_color().into());
    }

    base
}

/// Class for a toggle icon button. Delegates to the base `Button::Icon` Catalog for each state
/// so hover/press/focus behave normally and layers the selected look on top.
fn tool_toggle_class(selected: bool) -> theme::Button {
    theme::Button::Custom {
        active: Box::new(move |f, t| {
            apply_selected(
                button::Catalog::active(t, f, selected, &theme::Button::Icon),
                selected,
                t,
            )
        }),
        hovered: Box::new(move |f, t| {
            apply_selected(
                button::Catalog::hovered(t, f, selected, &theme::Button::Icon),
                selected,
                t,
            )
        }),
        pressed: Box::new(move |f, t| {
            apply_selected(
                button::Catalog::pressed(t, f, selected, &theme::Button::Icon),
                selected,
                t,
            )
        }),
        disabled: Box::new(move |t| {
            apply_selected(
                button::Catalog::disabled(t, &theme::Button::Icon),
                selected,
                t,
            )
        }),
    }
}
