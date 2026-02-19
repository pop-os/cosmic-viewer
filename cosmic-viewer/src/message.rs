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
    Quit,
    // Context page message passing
    Context(ContextMessage),
    // Viewport message passing
    Viewport(ViewportMessage),
    // Edit message passing
    Edit(EditMessage),
    Surface(cosmic::surface::Action),
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
