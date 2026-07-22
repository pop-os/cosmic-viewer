// SPDX-License-Identifier: GPL-3.0-only

use crate::{fl, key_binds::MenuAction, message::ViewerMessage};
use cosmic::{
    Core, Element,
    widget::{
        menu::{self, ItemHeight, ItemWidth, KeyBind},
        responsive_menu_bar,
    },
};
use std::{collections::HashMap, sync::LazyLock};

static MENU_ID: LazyLock<cosmic::widget::Id> =
    LazyLock::new(|| cosmic::widget::Id::new("responsive-menu"));

fn build_file_menu() -> Vec<menu::Item<MenuAction, String>> {
    let mut items = vec![menu::Item::Button(
        fl!("menu-open-file"),
        None,
        MenuAction::OpenFile,
    )];

    items.push(menu::Item::Button(
        fl!("menu-open-folder"),
        None,
        MenuAction::OpenFolder,
    ));

    items.push(menu::Item::Button(
        fl!("menu-open-containing"),
        None,
        MenuAction::OpenContaining,
    ));

    items.push(menu::Item::Divider);

    items.push(menu::Item::Button(fl!("menu-save"), None, MenuAction::Save));
    items.push(menu::Item::Button(
        fl!("menu-save-as"),
        None,
        MenuAction::SaveAs,
    ));

    items.push(menu::Item::Divider);

    items.push(menu::Item::Button(
        fl!("menu-set-wallpaper"),
        None,
        MenuAction::SetWallpaper,
    ));
    items.push(menu::Item::Button(
        fl!("menu-move-to-trash"),
        None,
        MenuAction::MoveToTrash,
    ));

    items.push(menu::Item::Divider);

    items.push(menu::Item::Button(fl!("menu-quit"), None, MenuAction::Quit));

    items
}

fn build_edit_menu(can_undo: bool, can_redo: bool) -> Vec<menu::Item<MenuAction, String>> {
    let undo_item = if can_undo {
        menu::Item::Button(fl!("menu-undo"), None, MenuAction::Undo)
    } else {
        menu::Item::ButtonDisabled(fl!("menu-undo"), None, MenuAction::Undo)
    };
    let redo_item = if can_redo {
        menu::Item::Button(fl!("menu-redo"), None, MenuAction::Redo)
    } else {
        menu::Item::ButtonDisabled(fl!("menu-redo"), None, MenuAction::Redo)
    };
    // Revert all is only meaningful when there's an edit to revert (something on the undo/redo
    // stacks); disable it otherwise, matching Undo/Redo.
    let revert_all_item = if can_undo || can_redo {
        menu::Item::Button(fl!("menu-revert-all"), None, MenuAction::RevertAll)
    } else {
        menu::Item::ButtonDisabled(fl!("menu-revert-all"), None, MenuAction::RevertAll)
    };

    vec![
        undo_item,
        redo_item,
        revert_all_item,
        menu::Item::Divider,
        menu::Item::Button(fl!("menu-cut"), None, MenuAction::Cut),
        menu::Item::Button(fl!("menu-copy"), None, MenuAction::Copy),
        menu::Item::Button(fl!("menu-paste"), None, MenuAction::Paste),
        menu::Item::Button(
            fl!("menu-copy-to-clipboard"),
            None,
            MenuAction::CopyToClipboard,
        ),
        menu::Item::Divider,
        menu::Item::Button(fl!("menu-rotate-left"), None, MenuAction::RotateLeft),
        menu::Item::Button(fl!("menu-rotate-right"), None, MenuAction::RotateRight),
    ]
}

fn build_view_menu() -> Vec<menu::Item<MenuAction, String>> {
    vec![
        menu::Item::Button(fl!("menu-zoom-in"), None, MenuAction::ZoomIn),
        menu::Item::Button(fl!("menu-fit-to-view"), None, MenuAction::FitToView),
        menu::Item::Button(fl!("menu-zoom-out"), None, MenuAction::ZoomOut),
        menu::Item::Button(fl!("menu-actual-size"), None, MenuAction::ActualSize),
        menu::Item::Divider,
        menu::Item::Button(fl!("menu-fullscreen"), None, MenuAction::Fullscreen),
        menu::Item::Divider,
        menu::Item::Button(fl!("menu-image-details"), None, MenuAction::ImageDetails),
    ]
}

// reason: cosmic's `into_element` requires the default-hasher HashMap, so this
// signature cannot be generalized over the BuildHasher.
#[allow(clippy::implicit_hasher)]
pub fn menu_bar<'a>(
    core: &Core,
    key_binds: &HashMap<KeyBind, MenuAction>,
    can_undo: bool,
    can_redo: bool,
) -> Element<'a, ViewerMessage> {
    let file_menu = build_file_menu();
    let edit_menu = build_edit_menu(can_undo, can_redo);
    let view_menu = build_view_menu();

    responsive_menu_bar()
        .item_height(ItemHeight::Dynamic(40))
        .item_width(ItemWidth::Uniform(320))
        .spacing(4.)
        .into_element(
            core,
            key_binds,
            MENU_ID.clone(),
            ViewerMessage::Surface,
            vec![
                (fl!("menu-file"), file_menu),
                (fl!("menu-edit"), edit_menu),
                (fl!("menu-view"), view_menu),
            ],
        )
}
