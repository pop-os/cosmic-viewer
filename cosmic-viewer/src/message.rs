use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMessage {
    About,
    ImageDetails,
}

#[derive(Debug, Clone)]
pub enum ViewerMessage {
    Copy,
    CopyToClipboard,
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
    Share,
    Print,
    Cancelled,
    Quit,
    Nav(NavMessage),
    Image(ImageMessage),
    // Context page message passing
    Context(ContextMessage),
    // Viewport message passing
    Viewport(ViewportMessage),
    // Edit message passing
    Edit(EditMessage),
    Surface(cosmic::surface::Action),
}

#[derive(Debug, Clone)]
pub enum NavMessage {
    ScanComplete(Vec<PathBuf>, Option<PathBuf>),
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
    RotateLeft,
    RotateRight,
    Undo,
    Redo,
    RevertAll,
}
