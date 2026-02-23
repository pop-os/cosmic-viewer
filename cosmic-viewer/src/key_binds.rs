use crate::{
    key_binds,
    message::{ContextMessage, EditMessage, ViewerMessage, ViewportMessage},
};
use cosmic::{
    iced::keyboard::{Key, Modifiers, key::Named},
    widget::menu::{
        Action,
        key_bind::{KeyBind, Modifier},
    },
};
use std::collections::HashMap;

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
    Share,
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
            MenuAction::Share => ViewerMessage::Share,
            MenuAction::Print => ViewerMessage::Print,
            MenuAction::Quit => ViewerMessage::Quit,
            // Context Actions
            MenuAction::ImageDetails => ViewerMessage::Context(ContextMessage::ImageDetails),
            MenuAction::About => ViewerMessage::Context(ContextMessage::About),
            // Viewport Actions
            MenuAction::ZoomIn => ViewerMessage::Viewport(ViewportMessage::ZoomIn),
            MenuAction::ZoomOut => ViewerMessage::Viewport(ViewportMessage::ZoomOut),
            MenuAction::FitToView => ViewerMessage::Viewport(ViewportMessage::FitToView),
            MenuAction::Fullscreen => ViewerMessage::Viewport(ViewportMessage::Fullscreen),
            // Edit Messages
            MenuAction::RotateLeft => ViewerMessage::Edit(EditMessage::RotateLeft),
            MenuAction::RotateRight => ViewerMessage::Edit(EditMessage::RotateRight),
            MenuAction::Undo => ViewerMessage::Edit(EditMessage::Undo),
            MenuAction::Redo => ViewerMessage::Edit(EditMessage::Redo),
            MenuAction::RevertAll => ViewerMessage::Edit(EditMessage::RevertAll),
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

    binds
}

pub fn keyboard_shortcut_handler(key: Key, modifiers: Modifiers) -> Option<ViewerMessage> {
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
