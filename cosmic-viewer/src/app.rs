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
        self, Alignment, Background, Color, Length, Point, Rectangle, Size, Subscription, Vector,
        alignment::Horizontal,
        clipboard, event, font,
        keyboard::key::{Key, Named},
    },
    iced_core::Border,
    iced_wgpu::graphics::text::font_system,
    iced_widget::scrollable::{AbsoluteOffset, scroll_to},
    task::future,
    widget::{
        self, Id, Space, button, column, container, divider, dropdown, horizontal_space, icon,
        menu::{KeyBind, menu_button},
        nav_bar, popover, row, text, vertical_space,
    },
};
use image::DynamicImage;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
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
        ShapePreview, TextPreview,
    },
    crop::{CropOperation, CropRatio, CropSelection},
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
    highlighter_stroke_size: f32,
    crop_ratio: CropRatio,
    crop_ratio_popup: bool,
    text_editing: bool,
    font_families: Vec<&'static str>,
    text_font_family: &'static str,
    text_font_index: Option<usize>,
    text_bold: bool,
    text_italic: bool,
    text_underline: bool,
    text_alignment: Horizontal,
    shape_popup: bool,
    stroke_popup: bool,
    selected_shape: AnnotateTool,
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

    /// Rebuild the working image.
    /// Used after dimension-changing operations like crop and rotate.
    fn rebuild_working_image(&mut self) {
        let original = self
            .nav
            .current()
            .and_then(|path| self.cache.get_full(path))
            .map(|cached| cached.image.clone());

        if let Some(mut base) = original {
            for op in self.viewport.operations() {
                if let Some(rotate) = op.as_any().downcast_ref::<RotateOperation>() {
                    base = match rotate.direction {
                        RotateDirection::Left => base.rotate270(),
                        RotateDirection::Right => base.rotate90(),
                    };
                } else if let Some(crop) = op.as_any().downcast_ref::<CropOperation>() {
                    base = base.crop_imm(
                        crop.region.x as u32,
                        crop.region.y as u32,
                        crop.region.width as u32,
                        crop.region.height as u32,
                    );
                }
            }

            if let Some(img) = self.viewport.working_image_mut() {
                *img = base;
            }

            self.viewport.rebuild_display();
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
                ToolbarItem::new(icon_btn(
                    "zoom-out-symbolic",
                    fl!("toolbar-zoom-out"),
                    ViewerMessage::Canvas(CanvasMessage::ZoomOut),
                ))
                .priority(ItemPriority::Essential),
            )
            .center(ToolbarItem::new(text::body(zoom_pct)).priority(ItemPriority::Essential))
            .center(
                ToolbarItem::new(icon_btn(
                    "zoom-in-symbolic",
                    fl!("toolbar-zoom-in"),
                    ViewerMessage::Canvas(CanvasMessage::ZoomIn),
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
            .view(|| ViewerMessage::ToolbarOverflowToggle)
    }

    fn build_crop_ratio_selector(&self) -> Element<'_, ViewerMessage> {
        let trigger = button::custom(
            row()
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
                .map(|size| size.height > size.width)
                .unwrap_or(false);

            let presets = CropRatio::presets();
            let mut list = column().spacing(4);

            for ratio in presets {
                let label = ratio.label(is_portrait).to_string();
                let is_selected = *ratio == self.crop_ratio;

                let item = row()
                    .push(text::body(label))
                    .push(if is_selected {
                        Element::from(icon::from_name("object-select-symbolic").size(16).icon())
                    } else {
                        Element::from(horizontal_space().width(16))
                    })
                    .align_y(Alignment::Center)
                    .width(Length::Shrink)
                    .spacing(8);

                list = list
                    .push(
                        button::custom(item)
                            .class(cosmic::theme::Button::Icon)
                            .on_press(ViewerMessage::Edit(EditMessage::CropRatio(*ratio))),
                    )
                    .align_x(Horizontal::Center)
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
                // TODO: Create libcosmic PR for Position::Top and replace when accepted.
                // The Point currently sets the popup to be on top of the toolbar centered
                // on the button.
                .position(popover::Position::Point(Point::new(-35.0, -10.0)))
                .on_close(ViewerMessage::Edit(EditMessage::CropRatioPopupToggle));
        }

        pop.into()
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

        let can_undo = self.viewport.can_undo();
        let can_redo = self.viewport.can_redo();

        let undo_btn = button::icon(icon::from_name("edit-undo-symbolic"))
            .tooltip(fl!("menu-undo"))
            .on_press_maybe(can_undo.then(|| ViewerMessage::Edit(EditMessage::Undo)));

        let redo_btn = button::icon(icon::from_name("edit-redo-symbolic"))
            .tooltip(fl!("menu-redo"))
            .on_press_maybe(can_redo.then(|| ViewerMessage::Edit(EditMessage::Redo)));

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
            .end(ToolbarItem::new(icon_btn(
                "window-close-symbolic",
                fl!("toolbar-cancel"),
                ViewerMessage::Edit(EditMessage::CropCancel),
            )))
            .end(ToolbarItem::new(icon_btn(
                "object-select-symbolic",
                fl!("toolbar-apply"),
                ViewerMessage::Edit(EditMessage::CropApply),
            )))
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
            .end(ToolbarItem::new(icon_btn(
                "insert-text-symbolic",
                fl!("text-tool"),
                ViewerMessage::Edit(EditMessage::AnnotateTool(AnnotateTool::Text)),
            )))
            .end(ToolbarItem::new(icon_btn(
                "pen-symbolic",
                fl!("drawing-pen"),
                ViewerMessage::Edit(EditMessage::AnnotateTool(AnnotateTool::draw_tools()[0])),
            )))
            .end(ToolbarItem::new(icon_btn(
                "insert-drawing-symbolic",
                fl!("drawing-highlighter"),
                ViewerMessage::Edit(EditMessage::AnnotateTool(AnnotateTool::Highlighter)),
            )))
            .end(ToolbarItem::new(self.build_shape_selector()))
            .end(ToolbarItem::new(icon_btn(
                "window-close-symbolic",
                fl!("toolbar-cancel"),
                ViewerMessage::Edit(EditMessage::AnnotateCancel),
            )))
            .center(ToolbarItem::new(self.build_stroke_selector()));

        let colors = AnnotateColor::presets();
        for color in &colors {
            let c = *color;
            let is_selected = c == self.annotate_color;
            toolbar = toolbar.center(ToolbarItem::new(
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
            ));
        }

        toolbar.view(|| ViewerMessage::ToolbarOverflowToggle)
    }

    fn build_text_format_popup(&self) -> Element<'_, ViewerMessage> {
        let font_dropdown = row()
            .push(icon::from_name("font-symbolic").size(16).icon())
            .push(dropdown(&self.font_families, self.text_font_index, |idx| {
                ViewerMessage::Edit(EditMessage::TextFontFamily(idx))
            }))
            .align_y(Alignment::Center)
            .spacing(4);

        // Bold, italic, underline as icon buttons
        let bold_btn = button::icon(icon::from_name("format-text-bold-symbolic"))
            .class(if self.text_bold {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Icon
            })
            .on_press(ViewerMessage::Edit(EditMessage::TextBold));

        let italic_btn = button::icon(icon::from_name("format-text-italic-symbolic"))
            .class(if self.text_italic {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Icon
            })
            .on_press(ViewerMessage::Edit(EditMessage::TextItalic));

        let underline_btn = button::icon(icon::from_name("format-text-underline-symbolic"))
            .class(if self.text_underline {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Icon
            })
            .on_press(ViewerMessage::Edit(EditMessage::TextUnderline));

        // Align buttons
        let align_left = button::icon(icon::from_name("format-justify-left-symbolic"))
            .class(if self.text_alignment == Horizontal::Left {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Icon
            })
            .on_press(ViewerMessage::Edit(EditMessage::TextAlignment(
                Horizontal::Left,
            )));
        let align_center = button::icon(icon::from_name("format-justify-center-symbolic"))
            .class(if self.text_alignment == Horizontal::Center {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Icon
            })
            .on_press(ViewerMessage::Edit(EditMessage::TextAlignment(
                Horizontal::Center,
            )));
        let align_right = button::icon(icon::from_name("format-justify-right-symbolic"))
            .class(if self.text_alignment == Horizontal::Right {
                cosmic::theme::Button::Suggested
            } else {
                cosmic::theme::Button::Icon
            })
            .on_press(ViewerMessage::Edit(EditMessage::TextAlignment(
                Horizontal::Right,
            )));

        let cancel_btn = button::icon(icon::from_name("window-close-symbolic"))
            .on_press(ViewerMessage::Edit(EditMessage::TextCancel));
        let apply_btn = button::icon(icon::from_name("object-select-symbolic"))
            .on_press(ViewerMessage::Edit(EditMessage::TextApply));

        container(
            column()
                .push(font_dropdown)
                .push(
                    row()
                        .push(align_left)
                        .push(align_center)
                        .push(align_right)
                        .spacing(4),
                )
                .push(
                    row()
                        .push(bold_btn)
                        .push(italic_btn)
                        .push(underline_btn)
                        //.push(horizontal_space())
                        .push(cancel_btn)
                        .push(apply_btn)
                        .spacing(4),
                )
                .spacing(8)
                .padding(12),
        )
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
        })
        .width(Length::Shrink)
        .into()
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

    fn build_shape_selector(&self) -> Element<'_, ViewerMessage> {
        let current_icon = self.selected_shape.icon_name();
        let trigger = button::icon(icon::from_name(current_icon))
            .on_press(ViewerMessage::Edit(EditMessage::ShapePopupToggle));

        let mut pop = popover(trigger);

        if self.shape_popup {
            let make_btn = |tool: AnnotateTool| -> Element<'_, ViewerMessage> {
                button::icon(icon::from_name(tool.icon_name()))
                    .class(if tool == self.annotate_tool {
                        cosmic::theme::Button::Suggested
                    } else {
                        cosmic::theme::Button::Icon
                    })
                    .on_press(ViewerMessage::Edit(EditMessage::AnnotateTool(tool)))
                    .into()
            };

            let list = column()
                .push(make_btn(AnnotateTool::Rectangle))
                .push(make_btn(AnnotateTool::Ellipse))
                .push(make_btn(AnnotateTool::Arrow))
                .push(make_btn(AnnotateTool::Line))
                .push(make_btn(AnnotateTool::Star))
                .push(make_btn(AnnotateTool::Polygon))
                .spacing(4);

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
                .on_close(ViewerMessage::Edit(EditMessage::ShapePopupToggle));
        }

        pop.into()
    }

    fn build_stroke_selector(&self) -> Element<'_, ViewerMessage> {
        let trigger = button::custom(
            row()
                .push(icon::from_name("stroke-width-symbolic").size(16).icon())
                .push(icon::from_name("pan-down-symbolic").size(12).icon())
                .align_y(Alignment::Center)
                .spacing(2),
        )
        .class(cosmic::theme::Button::Icon)
        .on_press(ViewerMessage::Edit(EditMessage::StrokePopupToggle));

        let mut pop = popover(trigger);

        if self.stroke_popup {
            let (labels, sizes, current) =
                if self.annotate_tool == AnnotateTool::Highlighter {
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

            let mut list = column().spacing(4);
            for (label, &size) in labels.iter().zip(sizes.iter()) {
                let is_selected = size == current;
                let item: Element<'_, ViewerMessage> = row()
                    .push(text::body(*label))
                    .push(if is_selected {
                        Element::from(icon::from_name("object-select-symbolic").size(16).icon())
                    } else {
                        Element::from(horizontal_space().width(16))
                    })
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .into();

                let idx = sizes.iter().position(|s| *s == size).unwrap();
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

        let grid = ImageGrid::new(Vec::new())
            .thumbnail_size(config.thumbnail_size.pixels())
            .on_activate(|idx| Action::App(ViewerMessage::Nav(NavMessage::GridActivate(idx))))
            .on_focus(|idx| Action::App(ViewerMessage::Nav(NavMessage::GridFocus(idx))))
            .on_scroll_request(|req| {
                Action::App(ViewerMessage::Nav(NavMessage::GridScroll(req.offset_y)))
            })
            .scrollable(scroll_id.clone());

        let families = load_font_families();
        let default_family = match cosmic::font::default().family {
            font::Family::Name(name) => name,
            _ => "Sans",
        };
        let font_index = families.iter().position(|&fam| fam == default_family);

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
            highlighter_stroke_size: 8.,
            crop_ratio: CropRatio::Custom,
            crop_ratio_popup: false,
            text_editing: false,
            font_families: load_font_families(),
            shape_popup: false,
            stroke_popup: false,
            selected_shape: AnnotateTool::Rectangle,
            text_font_family: default_family,
            text_font_index: font_index,
            text_bold: false,
            text_italic: false,
            text_underline: false,
            text_alignment: Horizontal::Left,
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
        } else if self.text_editing {
            let bounds = self.viewport.last_bounds();
            if let Some(preview) = self.viewport.preview_ref()
                && let Some(text) = preview.as_any().downcast_ref::<TextPreview>()
                && let Some(pos) = text.position
                && let Some(screen_pos) = self.viewport.image_to_screen(pos, bounds.get())
            {
                pop = pop
                    .popup(self.build_text_format_popup())
                    .position(popover::Position::Point(screen_pos));
            }
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
                    tracing::error!("Failed to open containing folder: {e}");
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
                    match &key {
                        Key::Named(Named::Backspace) => {
                            if let Some(preview) = preview {
                                preview.pop_char();
                            }
                        }
                        Key::Named(Named::Space) => {
                            if let Some(preview) = preview {
                                preview.push_char(' ');
                            }
                        }
                        Key::Named(Named::Enter) => {
                            self.viewport.apply_tool();
                            self.text_editing = false;
                            self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            self.viewport.set_preview(Some(Box::new(TextPreview::new(
                                self.annotate_color.0,
                                self.annotate_stroke_size * 8.0,
                                self.text_font_family,
                                self.text_bold,
                                self.text_italic,
                                self.text_underline,
                                self.text_alignment,
                            ))));
                        }
                        Key::Named(Named::Escape) => {
                            self.text_editing = false;
                            self.viewport.set_active_tool(Some(ToolKind::Annotate));
                            self.viewport.set_preview(Some(Box::new(TextPreview::new(
                                self.annotate_color.0,
                                self.annotate_stroke_size * 8.0,
                                self.text_font_family,
                                self.text_bold,
                                self.text_italic,
                                self.text_underline,
                                self.text_alignment,
                            ))));
                        }
                        _ => {
                            if let Some(txt) = &text
                                && preview.is_some()
                                && let Some(preview) = preview
                            {
                                for ch in txt.chars() {
                                    preview.push_char(ch);
                                }
                            }
                        }
                    }
                } else if let Some(msg) = keyboard_shortcut_handler(key, modifiers, text) {
                    return self.update(msg);
                }
            }
            ViewerMessage::WindowResized(size) => self.window_width = Some(size.width),
            ViewerMessage::Nav(msg) => match msg {
                NavMessage::ScanComplete(dir, images, select) => {
                    self.viewport.cancel_tool();
                    self.viewport.set_image(None, None);

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
                    if !self.text_editing
                        && let Some(path) = self.nav.select(idx)
                    {
                        let path = path.clone();
                        self.grid.set_focused(Some(idx));
                        self.grid.set_selected(vec![idx]);

                        // Clear any tool and any unsaved tool operations
                        self.viewport.cancel_tool();
                        self.viewport.revert_all();

                        if let Some(cached) = self.cache.get_full(&path) {
                            self.viewport.set_image(
                                Some(CanvasImage {
                                    handle: cached.handle,
                                    width: cached.width,
                                    height: cached.height,
                                }),
                                Some(cached.image.clone()),
                            );
                        } else {
                            self.viewport.set_image(None, None);
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
                                self.viewport.set_image(
                                    Some(CanvasImage {
                                        handle: cached.handle,
                                        width: cached.width,
                                        height: cached.height,
                                    }),
                                    Some(cached.image.clone()),
                                );
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
                    if self.context_menu_position.is_some() {
                        self.context_menu_position = None;
                        return Task::none();
                    }

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
                    if matches!(self.annotate_tool, AnnotateTool::Text)
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
                        self.viewport.set_active_tool(None);
                        self.viewport.set_preview(None);
                        // Restore working image from cache so rotation etc. still works
                        if let Some(path) = self.nav.current().cloned() {
                            if let Some(cached) = self.cache.get_full(&path) {
                                self.viewport.set_image(
                                    Some(CanvasImage {
                                        handle: cached.handle,
                                        width: cached.width,
                                        height: cached.height,
                                    }),
                                    Some(cached.image.clone()),
                                );
                            }
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
                                    self.annotate_stroke_size * 8.0,
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
                        // Grab the crop region from the live preview
                        let region = self
                            .viewport
                            .preview_mut()
                            .and_then(|preview| {
                                preview.as_any_mut().downcast_mut::<CropSelection>()
                            })
                            .map(|sel| sel.region);

                        if let Some(region) = region {
                            // Transform all existing operations
                            for op in self.viewport.operations_mut() {
                                op.transform_crop(region);
                            }

                            // Commit the crop operation to the undo stack
                            self.viewport.apply_tool();

                            // Crop the base image
                            if let Some(img) = self.viewport.working_image_mut() {
                                *img = img.crop_imm(
                                    region.x as u32,
                                    region.y as u32,
                                    region.width as u32,
                                    region.height as u32,
                                );
                            }
                            self.viewport.rebuild_display();
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
                    EditMessage::CropRatioPopupToggle => {
                        self.crop_ratio_popup = !self.crop_ratio_popup
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
                        self.viewport.rebuild_display();
                    }
                    EditMessage::RotateRight => {
                        self.viewport.cancel_tool();
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
                        self.viewport.rebuild_display();
                    }
                    EditMessage::ShapePopupToggle => self.shape_popup = !self.shape_popup,
                    EditMessage::StrokePopupToggle => self.stroke_popup = !self.stroke_popup,
                    EditMessage::TextBold => {
                        self.text_bold = !self.text_bold;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            text.bold = self.text_bold;
                        }
                    }
                    EditMessage::TextItalic => {
                        self.text_italic = !self.text_italic;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            text.italic = self.text_italic;
                        }
                    }
                    EditMessage::TextUnderline => {
                        self.text_underline = !self.text_underline;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            text.underline = self.text_underline;
                        }
                    }
                    EditMessage::TextAlignment(alignment) => {
                        self.text_alignment = alignment;
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            text.alignment = self.text_alignment;
                        }
                    }
                    EditMessage::TextFontFamily(idx) => {
                        self.text_font_index = Some(idx);
                        if let Some(preview) = self.viewport.preview_mut()
                            && let Some(text) = preview.as_any_mut().downcast_mut::<TextPreview>()
                        {
                            text.font_family = self.font_families[idx];
                        }
                    }
                    EditMessage::TextCancel => {
                        self.viewport.cancel_tool();
                        self.text_editing = false;
                    }
                    EditMessage::TextApply => {
                        self.viewport.apply_tool();
                        self.text_editing = false;
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
        Subscription::batch([event::listen_with(|event, _status, _id| match event {
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
        })])
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

fn load_font_families() -> Vec<&'static str> {
    let mut font_sys = font_system().write().expect("Read font system");
    let db = font_sys.raw().db();

    let mut families: Vec<String> = db
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    families.sort();

    families
        .into_iter()
        .map(|name| &*Box::leak(name.into_boxed_str()))
        .collect()
}
