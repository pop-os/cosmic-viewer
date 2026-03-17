use cosmic::{
    iced::{
        Size,
        alignment::Horizontal,
        keyboard::{Key, Modifiers},
    },
    iced_core::SmolStr,
};
use std::path::PathBuf;
use viewer_canvas::CanvasMessage;
use viewer_tools::{
    annotate::{AnnotateColor, AnnotateTool},
    crop::CropRatio,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMessage {
    About,
    ImageDetails,
}

#[derive(Debug, Clone)]
pub enum ViewerMessage {
    Copy,
    CopyToClipboard,
    CopyFilePath,
    Cut,
    // Path resolved from CLI arg or dialog
    Open(PathBuf),
    OpenFileDialog,
    OpenFolderDialog,
    OpenRecent(usize),
    OpenContaining,
    Paste,
    CloseFile,
    Save,
    SaveAs,
    SavedAs(PathBuf),
    Share,
    Print,
    Cancelled,
    Quit,
    Nav(NavMessage),
    ToolbarOverflowToggle,
    WindowResized(Size),
    KeyPressed(Key, Modifiers, Option<SmolStr>),
    Image(ImageMessage),
    // Context page message passing
    Context(ContextMessage),
    // Viewport message passing
    Canvas(CanvasMessage),
    // Edit message passing
    Edit(EditMessage),
    Surface(cosmic::surface::Action),
}

#[derive(Debug, Clone)]
pub enum NavMessage {
    ScanComplete(PathBuf, Vec<PathBuf>, Option<PathBuf>),
    GridActivate(usize),
    GridFocus(usize),
    GridScroll(f32),
}

#[derive(Debug, Clone)]
pub enum ImageMessage {
    // path, width, height (handle is in cache)
    ThumbnailReady(PathBuf, u32, u32),
    ImageReady(PathBuf),
    LoadError(PathBuf),
}

#[derive(Debug, Clone)]
pub enum ViewportMessage {
    ZoomIn,
    ZoomOut,
    FitToView,
    Fullscreen,
}

#[derive(Debug, Clone)]
pub enum EditMessage {
    Annotate,
    AnnotateCancel,
    AnnotateApply,
    AnnotateTool(AnnotateTool),
    AnnotateColor(AnnotateColor),
    AnnotateStroke(usize),
    CropRatioPopupToggle,
    Crop,
    CropApply,
    CropCancel,
    CropRatio(CropRatio),
    RotateLeft,
    RotateRight,
    ShapePopupToggle,
    TextBold,
    TextItalic,
    TextUnderline,
    TextFontFamily(usize),
    TextAlignment(Horizontal),
    TextApply,
    TextCancel,
    Undo,
    Redo,
    RevertAll,
}
