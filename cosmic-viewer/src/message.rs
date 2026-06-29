use cosmic::{
    iced::{
        Size,
        alignment::Horizontal,
        keyboard::{Key, Modifiers},
    },
    widget::{ToastId, color_picker::ColorPickerUpdate},
};
use smol_str::SmolStr;
use std::{path::PathBuf, sync::Arc};
use trash::TrashItem;
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
pub enum WallpaperTarget {
    All,
    Output(String),
}

#[derive(Debug, Clone)]
pub enum ViewerMessage {
    Copy,
    CopyToClipboard,
    CopyFilePath,
    Cut,
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
    Print,
    SetWallpaper,
    SetWallpaperOn(PathBuf, WallpaperTarget),
    CloseWallpaperDialog,
    WallpaperResult(Result<(), String>),
    CloseToast(ToastId),
    MoveToTrash,
    UndoTrash(Arc<[PathBuf]>),
    UndoTrashStart(Vec<TrashItem>),
    TrashResult(Result<(), String>),
    Cancelled,
    Quit,
    Nav(NavMessage),
    WindowResized(Size),
    KeyPressed(Key, Modifiers, Option<SmolStr>),
    Image(ImageMessage),
    Context(ContextMessage),
    Canvas(CanvasMessage),
    Edit(EditMessage),
    Surface(cosmic::surface::Action),
    WatcherEvent(crate::watcher::WatcherEvent),
    WatcherRescan,
    TextPaste(String),
}

#[derive(Debug, Clone)]
pub enum NavMessage {
    ScanComplete(PathBuf, Vec<PathBuf>, Option<PathBuf>),
    DirectoryRefreshed(Vec<PathBuf>),
    NavThumbnailShow(usize),
    NavThumbnailHide(usize),
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
    StrokePopupToggle,
    ColorPicker(ColorPickerUpdate),
    TextBold,
    TextItalic,
    TextUnderline,
    TextFontSize(usize),
    TextFontFamily(usize),
    TextAlignment(Horizontal),
    TextApply,
    TextCancel,
    ToggleTextFormatMenu,
    ToggleMoveMode,
    Undo,
    Redo,
    RevertAll,
}
