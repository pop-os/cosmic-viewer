use cosmic::widget::image::Handle;
use image::DynamicImage;
use lru::LruCache;
use std::{
    collections::HashSet,
    num::NonZeroUsize,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct CachedImage {
    pub handle: Handle,
    pub image: DynamicImage,
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Debug for CachedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct ImageCache {
    full_images: Arc<Mutex<LruCache<PathBuf, CachedImage>>>,
    thumbnails: Arc<Mutex<LruCache<PathBuf, Handle>>>,
    pending: Arc<Mutex<HashSet<PathBuf>>>,
    pending_thumbnails: Arc<Mutex<HashSet<PathBuf>>>,
}

impl ImageCache {
    #[must_use]
    pub fn new(full_capacity: usize, thumbnail_capacity: usize) -> Self {
        // `max(1)` guarantees a non-zero capacity; the `unwrap_or` branch is
        // therefore unreachable and avoids a panicking path entirely.
        let full = NonZeroUsize::new(full_capacity.max(1)).unwrap_or(NonZeroUsize::MIN);
        let thumbnail = NonZeroUsize::new(thumbnail_capacity.max(1)).unwrap_or(NonZeroUsize::MIN);
        Self {
            full_images: Arc::new(Mutex::new(LruCache::new(full))),
            thumbnails: Arc::new(Mutex::new(LruCache::new(thumbnail))),
            pending: Arc::new(Mutex::new(HashSet::new())),
            pending_thumbnails: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(20, 1000)
    }

    pub fn resize(&self, new_capacity: usize) {
        // `max(1)` guarantees a non-zero capacity; the `unwrap_or` branch is
        // therefore unreachable and avoids a panicking path entirely.
        let capacity = NonZeroUsize::new(new_capacity.max(1)).unwrap_or(NonZeroUsize::MIN);
        if let Ok(mut cache) = self.full_images.lock() {
            cache.resize(capacity);
        }
    }

    #[must_use]
    pub fn get_full(&self, path: &PathBuf) -> Option<CachedImage> {
        self.full_images.lock().ok()?.get(path).cloned()
    }

    pub fn insert_full(&self, path: PathBuf, image: CachedImage) {
        self.clear_pending(&path);
        if let Ok(mut cache) = self.full_images.lock() {
            cache.put(path, image);
        }
    }

    pub fn remove_full(&self, path: &PathBuf) {
        if let Ok(mut cache) = self.full_images.lock() {
            cache.pop(path);
        }
    }

    #[must_use]
    pub fn get_thumbnail(&self, path: &PathBuf) -> Option<Handle> {
        self.thumbnails.lock().ok()?.get(path).cloned()
    }

    pub fn insert_thumbnail(&self, path: PathBuf, handle: Handle) {
        self.clear_pending_thumbnail(&path);
        if let Ok(mut cache) = self.thumbnails.lock() {
            cache.put(path, handle);
        }
    }

    pub fn remove_thumbnail(&self, path: &PathBuf) {
        if let Ok(mut cache) = self.thumbnails.lock() {
            cache.pop(path);
        }
    }

    #[must_use]
    pub fn pending_thumbnail_count(&self) -> usize {
        self.pending_thumbnails.lock().map_or(0, |set| set.len())
    }

    #[must_use]
    pub fn is_thumbnail_pending(&self, path: &PathBuf) -> bool {
        self.pending_thumbnails
            .lock()
            .is_ok_and(|set| set.contains(path))
    }

    pub fn set_thumbnail_pending(&self, path: PathBuf) {
        if let Ok(mut set) = self.pending_thumbnails.lock() {
            set.insert(path);
        }
    }

    pub fn clear_pending_thumbnail(&self, path: &PathBuf) {
        if let Ok(mut set) = self.pending_thumbnails.lock() {
            set.remove(path);
        }
    }

    #[must_use]
    pub fn is_pending(&self, path: &PathBuf) -> bool {
        self.pending.lock().is_ok_and(|set| set.contains(path))
    }

    pub fn set_pending(&self, path: PathBuf) {
        if let Ok(mut set) = self.pending.lock() {
            set.insert(path);
        }
    }

    pub fn clear_pending(&self, path: &PathBuf) {
        if let Ok(mut set) = self.pending.lock() {
            set.remove(path);
        }
    }

    pub fn clear_thumbnails(&self) {
        if let Ok(mut cache) = self.thumbnails.lock() {
            cache.clear();
        }
        if let Ok(mut set) = self.pending_thumbnails.lock() {
            set.clear();
        }
    }

    pub fn clear(&self) {
        if let Ok(mut cache) = self.full_images.lock() {
            cache.clear();
        }

        if let Ok(mut cache) = self.thumbnails.lock() {
            cache.clear();
        }

        if let Ok(mut set) = self.pending.lock() {
            set.clear();
        }

        if let Ok(mut set) = self.pending_thumbnails.lock() {
            set.clear();
        }
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::with_defaults()
    }
}
