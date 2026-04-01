use crate::message::{ContextMessage, EditMessage, ViewerMessage};
use cosmic::{
    iced::keyboard::{Key, Modifiers, key::Named},
    iced_core::SmolStr,
    widget::menu::{
        Action,
        key_bind::{KeyBind, Modifier},
    },
};
use std::collections::HashMap;
use viewer_canvas::CanvasMessage;

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
    FitToView,
    Fullscreen,
    SetWallpaper,
    MoveToTrash,
    DeletePermanently,
    About,
}

impl MenuAction {
    pub fn message(self) -> ViewerMessage {
        match self {
            // Basic App Actions
            MenuAction::OpenFile => ViewerMessage::OpenFileDialog,
            MenuAction::OpenFolder => ViewerMessage::OpenFolderDialog,
            MenuAction::OpenRecent(idx) => ViewerMessage::OpenRecent(idx),
            MenuAction::OpenContaining => ViewerMessage::OpenContaining,
            MenuAction::CloseFile => ViewerMessage::CloseFile,
            MenuAction::Copy => ViewerMessage::Copy,
            MenuAction::CopyToClipboard => ViewerMessage::CopyToClipboard,
            MenuAction::Cut => ViewerMessage::Cut,
            MenuAction::Paste => ViewerMessage::Paste,
            MenuAction::Save => ViewerMessage::Save,
            MenuAction::SaveAs => ViewerMessage::SaveAs,
            MenuAction::Print => ViewerMessage::Print,
            MenuAction::Quit => ViewerMessage::Quit,
            // Context Actions
            MenuAction::ImageDetails => ViewerMessage::Context(ContextMessage::ImageDetails),
            MenuAction::About => ViewerMessage::Context(ContextMessage::About),
            // Viewport Actions
            MenuAction::ZoomIn => ViewerMessage::Canvas(CanvasMessage::ZoomIn),
            MenuAction::ZoomOut => ViewerMessage::Canvas(CanvasMessage::ZoomOut),
            MenuAction::FitToView => ViewerMessage::Canvas(CanvasMessage::FitToView),
            MenuAction::Fullscreen => ViewerMessage::Canvas(CanvasMessage::Fullscreen),
            // Edit Messages
            MenuAction::RotateLeft => ViewerMessage::Edit(EditMessage::RotateLeft),
            MenuAction::RotateRight => ViewerMessage::Edit(EditMessage::RotateRight),
            MenuAction::Undo => ViewerMessage::Edit(EditMessage::Undo),
            MenuAction::Redo => ViewerMessage::Edit(EditMessage::Redo),
            MenuAction::RevertAll => ViewerMessage::Edit(EditMessage::RevertAll),
            MenuAction::SetWallpaper => ViewerMessage::SetWallpaper,
            MenuAction::MoveToTrash => ViewerMessage::MoveToTrash,
            MenuAction::DeletePermanently => ViewerMessage::DeletePermanently,
        }
    }
}

impl Action for MenuAction {
    type Message = ViewerMessage;

    fn message(&self) -> ViewerMessage {
        (*self).message()
    }
}

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
            modifiers: vec![],
            key: Key::Named(Named::F11),
        },
        MenuAction::Fullscreen,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![],
            key: Key::Character("+".into()),
        },
        MenuAction::ZoomIn,
    );

    binds.insert(
        KeyBind {
            modifiers: vec![],
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
        key: key.clone(),
    };

    init_keybinds()
        .get(&key_bind)
        .map(|action| action.message())
}
