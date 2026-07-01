use crate::message::{ContextMessage, EditMessage, ViewerMessage};
use cosmic::{
    iced::keyboard::{Key, Modifiers},
    widget::menu::{
        Action,
        key_bind::{KeyBind, Modifier},
    },
};
use smol_str::SmolStr;
use std::{collections::HashMap, sync::OnceLock};
use viewer_canvas::CanvasMessage;

static KEYBINDS: OnceLock<HashMap<KeyBind, MenuAction>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    OpenFile,
    OpenFolder,
    OpenRecent(usize),
    OpenContaining,
    CloseFile,
    Save,
    SaveAs,
    ImageDetails,
    Print,
    Quit,
    Undo,
    Redo,
    RevertAll,
    Cut,
    Copy,
    Paste,
    CopyToClipboard,
    RotateLeft,
    RotateRight,
    ZoomIn,
    ZoomOut,
    ActualSize,
    FitToView,
    Fullscreen,
    SetWallpaper,
    MoveToTrash,
    About,
}

impl MenuAction {
    #[must_use]
    pub const fn message(self) -> ViewerMessage {
        match self {
            // Basic App Actions
            Self::OpenFile => ViewerMessage::OpenFileDialog,
            Self::OpenFolder => ViewerMessage::OpenFolderDialog,
            Self::OpenRecent(idx) => ViewerMessage::OpenRecent(idx),
            Self::OpenContaining => ViewerMessage::OpenContaining,
            Self::CloseFile => ViewerMessage::CloseFile,
            Self::Copy => ViewerMessage::Copy,
            Self::CopyToClipboard => ViewerMessage::CopyToClipboard,
            Self::Cut => ViewerMessage::Cut,
            Self::Paste => ViewerMessage::Paste,
            Self::Save => ViewerMessage::Save,
            Self::SaveAs => ViewerMessage::SaveAs,
            Self::Print => ViewerMessage::Print,
            Self::Quit => ViewerMessage::Quit,
            // Context Actions
            Self::ImageDetails => ViewerMessage::Context(ContextMessage::ImageDetails),
            Self::About => ViewerMessage::Context(ContextMessage::About),
            // Viewport Actions
            Self::ZoomIn => ViewerMessage::Canvas(CanvasMessage::ZoomIn),
            Self::ZoomOut => ViewerMessage::Canvas(CanvasMessage::ZoomOut),
            Self::ActualSize => ViewerMessage::Canvas(CanvasMessage::ActualSize),
            Self::FitToView => ViewerMessage::Canvas(CanvasMessage::FitToView),
            Self::Fullscreen => ViewerMessage::Canvas(CanvasMessage::Fullscreen),
            // Edit Messages
            Self::RotateLeft => ViewerMessage::Edit(EditMessage::RotateLeft),
            Self::RotateRight => ViewerMessage::Edit(EditMessage::RotateRight),
            Self::Undo => ViewerMessage::Edit(EditMessage::Undo),
            Self::Redo => ViewerMessage::Edit(EditMessage::Redo),
            Self::RevertAll => ViewerMessage::Edit(EditMessage::RevertAll),
            Self::SetWallpaper => ViewerMessage::SetWallpaper,
            Self::MoveToTrash => ViewerMessage::MoveToTrash,
        }
    }
}

impl Action for MenuAction {
    type Message = ViewerMessage;

    fn message(&self) -> ViewerMessage {
        (*self).message()
    }
}

// reason: flat keybinding registration table; one insert per shortcut.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn init_keybinds() -> HashMap<KeyBind, MenuAction> {
    let mut binds = HashMap::new();

    // File Ops
    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("o".into()),
        },
        MenuAction::OpenFile,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("o".into()),
        },
        MenuAction::OpenFolder,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("q".into()),
        },
        MenuAction::Quit,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("c".into()),
        },
        MenuAction::OpenContaining,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("w".into()),
        },
        MenuAction::CloseFile,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("s".into()),
        },
        MenuAction::Save,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("s".into()),
        },
        MenuAction::SaveAs,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("d".into()),
        },
        MenuAction::ImageDetails,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("p".into()),
        },
        MenuAction::Print,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("q".into()),
        },
        MenuAction::Quit,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("0".into()),
        },
        MenuAction::FitToView,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("1".into()),
        },
        MenuAction::ActualSize,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("f".into()),
        },
        MenuAction::Fullscreen,
    );

    // ZoomIn: canonical Ctrl+= plus aliases for the "+" key and Shift variants,
    // so Ctrl++ (physically Ctrl+Shift+=) also zooms in.
    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("=".into()),
        },
        MenuAction::ZoomIn,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("=".into()),
        },
        MenuAction::ZoomIn,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("+".into()),
        },
        MenuAction::ZoomIn,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("+".into()),
        },
        MenuAction::ZoomIn,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("-".into()),
        },
        MenuAction::ZoomOut,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("z".into()),
        },
        MenuAction::Undo,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("y".into()),
        },
        MenuAction::Redo,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("z".into()),
        },
        MenuAction::RevertAll,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("x".into()),
        },
        MenuAction::Cut,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("c".into()),
        },
        MenuAction::Copy,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("c".into()),
        },
        MenuAction::CopyToClipboard,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("v".into()),
        },
        MenuAction::Paste,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl],
            key: Key::Character("r".into()),
        },
        MenuAction::RotateRight,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: Key::Character("r".into()),
        },
        MenuAction::RotateLeft,
    );

    binds
}

pub fn keyboard_shortcut_handler(
    key: Key,
    modifiers: Modifiers,
    _text: Option<SmolStr>,
) -> Option<ViewerMessage> {
    let mut mods = vec![];

    if modifiers.control() {
        mods.push(Modifier::Ctrl);
    }

    if modifiers.shift() {
        mods.push(Modifier::Shift);
    }

    if modifiers.alt() {
        mods.push(Modifier::Alt);
    }

    if modifiers.logo() {
        mods.push(Modifier::Super);
    }

    let key_bind = KeyBind {
        modifiers: mods,
        key,
    };

    KEYBINDS
        .get_or_init(init_keybinds)
        .get(&key_bind)
        .map(cosmic::widget::menu::Action::message)
}
