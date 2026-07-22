// SPDX-License-Identifier: GPL-3.0-only

use cosmic::widget::icon;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IconCacheKey {
    name: &'static str,
}

pub struct IconCache {
    cache: HashMap<IconCacheKey, icon::Handle>,
}

impl IconCache {
    pub fn new() -> Self {
        let mut cache = HashMap::new();

        macro_rules! bundle {
            ($name:expr) => {
                let data: &'static [u8] =
                    include_bytes!(concat!("../../res/icons/", $name, ".svg"));
                cache.insert(
                    IconCacheKey { name: $name },
                    icon::from_svg_bytes(data).symbolic(true),
                );
            };
        }

        bundle!("markup-symbolic");
        bundle!("image-crop-rotate-symbolic");
        bundle!("view-fit-symbolic");
        bundle!("view-actual-size-symbolic");
        bundle!("pan-down-symbolic");
        bundle!("ratios-symbolic");
        bundle!("insert-text2-symbolic");
        bundle!("insert-drawing-symbolic");
        bundle!("text-highlight-symbolic");
        bundle!("stroke-width-symbolic");
        bundle!("format-text-bold-symbolic");
        bundle!("format-text-italic-symbolic");
        bundle!("format-text-underline-symbolic");

        Self { cache }
    }

    pub fn get(&mut self, name: &'static str) -> icon::Handle {
        self.cache
            .entry(IconCacheKey { name })
            .or_insert_with(|| icon::from_name(name).handle())
            .clone()
    }
}
