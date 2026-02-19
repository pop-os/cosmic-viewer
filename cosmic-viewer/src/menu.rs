use crate::{fl, key_binds::MenuAction, message::ViewerMessage};
use cosmic::{
    Core, Element,
    widget::{
        menu::{self, ItemHeight, ItemWidth, KeyBind},
        responsive_menu_bar,
    },
};
use std::{collections::HashMap, path::Path, sync::LazyLock};

static MENU_ID: LazyLock<cosmic::widget::Id> =
    LazyLock::new(|| cosmic::widget::Id::new("responsive-menu"));

fn build_file_menu(recent_folders: &[String]) -> Vec<menu::Item<MenuAction, String>> {
    let mut items = vec![menu::Item::Button(
        fl!("menu-open-file"),
        None,
        MenuAction::OpenFile,
    )];

    if !recent_folders.is_empty() {
        let folder_items: Vec<menu::Item<MenuAction, String>> = recent_folders
            .iter()
            .enumerate()
            .map(|(idx, folder)| {
                let display_name = Path::new(folder)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(folder)
                    .to_string();

                menu::Item::Button(display_name, None, MenuAction::OpenRecent(idx))
            })
            .collect();

        items.push(menu::Item::Folder(fl!("menu-open-recent"), folder_items));
    }

    items.push(menu::Item::Button(
        fl!("menu-open-folder"),
        None,
        MenuAction::OpenFolder,
    ));
    items.push(menu::Item::Button(
        fl!("menu-close-file"),
        None,
        MenuAction::CloseFile,
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
        fl!("menu-image-details"),
        None,
        MenuAction::ImageDetails,
    ));
    items.push(menu::Item::Button(
        fl!("menu-share"),
        None,
        MenuAction::Share,
    ));
    items.push(menu::Item::Button(
        fl!("menu-print"),
        None,
        MenuAction::Print,
    ));

    items.push(menu::Item::Divider);

    items.push(menu::Item::Button(fl!("menu-quit"), None, MenuAction::Quit));

    items
}

fn build_edit_menu() -> Vec<menu::Item<MenuAction, String>> {
    vec![
        menu::Item::Button(fl!("menu-undo"), None, MenuAction::Undo),
        menu::Item::Button(fl!("menu-redo"), None, MenuAction::Redo),
        menu::Item::Button(fl!("menu-revert-all"), None, MenuAction::RevertAll),
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
        menu::Item::Button(fl!("menu-rotate-right"), None, MenuAction::RotateLeft),
    ]
}

fn build_view_menu() -> Vec<menu::Item<MenuAction, String>> {
    vec![
        menu::Item::Button(fl!("menu-zoom-in"), None, MenuAction::ZoomIn),
        menu::Item::Button(fl!("menu-fit-to-view"), None, MenuAction::FitToView),
        menu::Item::Button(fl!("menu-zoom-out"), None, MenuAction::ZoomOut),
        menu::Item::Divider,
        menu::Item::Button(fl!("menu-fullscreen"), None, MenuAction::Fullscreen),
    ]
}

pub fn menu_bar<'a>(
    core: &Core,
    key_binds: &HashMap<KeyBind, MenuAction>,
    recent_folders: &[String],
) -> Element<'a, ViewerMessage> {
    let file_menu = build_file_menu(recent_folders);
    let edit_menu = build_edit_menu();
    let view_menu = build_view_menu();

    responsive_menu_bar()
        .item_height(ItemHeight::Dynamic(40))
        .item_width(ItemWidth::Uniform(250))
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
