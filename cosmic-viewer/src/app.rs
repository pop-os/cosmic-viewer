use crate::{
    fl,
    key_binds::{self, MenuAction},
    menu::menu_bar,
    message::{ContextMessage, EditMessage, ViewerMessage, ViewportMessage},
};
use cosmic::{
    Action, Application, ApplicationExt, Core, Element, Task,
    app::context_drawer,
    iced::{Length, alignment::Horizontal},
    task::future,
    widget::{Id, column, menu::KeyBind, nav_bar, text, vertical_space},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub struct CosmicViewer {
    core: Core,
    key_binds: HashMap<KeyBind, MenuAction>,
    nav_model: nav_bar::Model,
}

impl CosmicViewer {
    fn build_nav_model(/*active_image: ImageView*/) -> nav_bar::Model {
        let mut builder = nav_bar::Model::builder();

        // TODO Get Image Thumbnails

        let mut model = builder.build();

        // TODO Set the correct thumbnail active

        model
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

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let mut tasks = vec![];

        let mut viewer = Self {
            core,
            key_binds: key_binds::init_keybinds(),
            nav_model: Self::build_nav_model(),
        };

        tasks
            .push(viewer.set_window_title(fl!("app-title"), viewer.core.main_window_id().unwrap()));

        (viewer, Task::batch(tasks))
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![menu_bar(&self.core, &self.key_binds, &vec![])]
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, _id: nav_bar::Id) -> Task<Action<Self::Message>> {
        // TODO: Activate the right thumbnail

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        column()
            .push(vertical_space().height(Length::Fill))
            .push(text("placeholder").center())
            .push(vertical_space().height(Length::Fill))
            .align_x(Horizontal::Center)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        let mut tasks = vec![];

        match message {
            ViewerMessage::Copy => {}
            ViewerMessage::CopyToClipboard => {}
            ViewerMessage::Cut => {}
            ViewerMessage::OpenFileDialog => {}
            ViewerMessage::OpenFolderDialog => {}
            ViewerMessage::OpenRecent(_idx) => {}
            ViewerMessage::OpenContaining => {}
            ViewerMessage::Paste => {}
            ViewerMessage::CloseFile => {}
            ViewerMessage::Save => {}
            ViewerMessage::SaveAs => {}
            ViewerMessage::Share => {}
            ViewerMessage::Print => {}
            ViewerMessage::Quit => {}
            ViewerMessage::Context(_msg) => {}
            ViewerMessage::Viewport(_msg) => {}
            ViewerMessage::Edit(_msg) => {}
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
}
