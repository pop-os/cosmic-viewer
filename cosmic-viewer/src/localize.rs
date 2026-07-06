// SPDX-License-Identifier: GPL-3.0-only

use i18n_embed::{
    DesktopLanguageRequester,
    fluent::{FluentLanguageLoader, fluent_language_loader},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../i18n"]
struct Localizations;

pub static LANGUAGE_LOADER: std::sync::LazyLock<FluentLanguageLoader> =
    std::sync::LazyLock::new(|| {
        let loader: FluentLanguageLoader = fluent_language_loader!();
        let requested_languages = DesktopLanguageRequester::requested_languages();
        let _result = i18n_embed::select(&loader, &Localizations, &requested_languages);
        loader
    });

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::localize::LANGUAGE_LOADER, $message_id)
    }};
    ($message_id:literal, $($arg:tt)*) => {{
        i18n_embed_fl::fl!($crate::localize::LANGUAGE_LOADER, $message_id, $($arg)*)
    }};
}
