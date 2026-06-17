use cosmic::widget::image::Handle;
use image::DynamicImage;
use std::path::PathBuf;
use viewer_core::{CachedImage, ImageCache};

fn dummy_cached(w: u32, h: u32) -> CachedImage {
    let img = DynamicImage::new_rgba8(w, h);
    let rgba = img.to_rgba8();
    let handle = Handle::from_rgba(w, h, rgba.into_raw());
    CachedImage {
        handle,
        image: img,
        width: w,
        height: h,
    }
}

fn dummy_handle() -> Handle {
    Handle::from_rgba(1, 1, vec![0, 0, 0, 255])
}

#[test]
fn insert_full_and_thumbnail_both_retrievable() {
    let cache = ImageCache::with_defaults();
    let path = PathBuf::from("/tmp/cache-test/a.jpg");

    cache.insert_full(path.clone(), dummy_cached(200, 150));
    cache.insert_thumbnail(path.clone(), dummy_handle());

    let full = cache.get_full(&path).expect("full should be cached");
    assert_eq!(full.width, 200);
    assert_eq!(full.height, 150);

    assert!(cache.get_thumbnail(&path).is_some(), "thumb cached");
}

#[test]
fn lru_eviction_full_images() {
    let cache = ImageCache::new(3, 100);

    for i in 0..5 {
        let path = PathBuf::from(format!("/tmp/cache-evict/full_{i}.jpg"));
        cache.insert_full(path, dummy_cached(100, 100));
    }

    // First two should have been evicted
    assert!(
        cache
            .get_full(&PathBuf::from("/tmp/cache-evict/full_0.jpg"))
            .is_none()
    );
    assert!(
        cache
            .get_full(&PathBuf::from("/tmp/cache-evict/full_1.jpg"))
            .is_none()
    );
    // Last three present
    assert!(
        cache
            .get_full(&PathBuf::from("/tmp/cache-evict/full_2.jpg"))
            .is_some()
    );
    assert!(
        cache
            .get_full(&PathBuf::from("/tmp/cache-evict/full_3.jpg"))
            .is_some()
    );
    assert!(
        cache
            .get_full(&PathBuf::from("/tmp/cache-evict/full_4.jpg"))
            .is_some()
    );
}

#[test]
fn lru_eviction_thumbnails() {
    let cache = ImageCache::new(100, 3);

    for i in 0..5 {
        let path = PathBuf::from(format!("/tmp/cache-evict/thumb_{i}.jpg"));
        cache.insert_thumbnail(path, dummy_handle());
    }

    assert!(
        cache
            .get_thumbnail(&PathBuf::from("/tmp/cache-evict/thumb_0.jpg"))
            .is_none()
    );
    assert!(
        cache
            .get_thumbnail(&PathBuf::from("/tmp/cache-evict/thumb_1.jpg"))
            .is_none()
    );
    assert!(
        cache
            .get_thumbnail(&PathBuf::from("/tmp/cache-evict/thumb_4.jpg"))
            .is_some()
    );
}

#[test]
fn pending_state_cleared_on_insert() {
    let cache = ImageCache::with_defaults();
    let path = PathBuf::from("/tmp/cache-pending/a.jpg");

    cache.set_pending(path.clone());
    assert!(cache.is_pending(&path));

    cache.insert_full(path.clone(), dummy_cached(10, 10));
    assert!(!cache.is_pending(&path));
}

#[test]
fn thumbnail_pending_cleared_on_insert() {
    let cache = ImageCache::with_defaults();
    let path = PathBuf::from("/tmp/cache-pending/b.jpg");

    cache.set_thumbnail_pending(path.clone());
    assert!(cache.is_thumbnail_pending(&path));

    cache.insert_thumbnail(path.clone(), dummy_handle());
    assert!(!cache.is_thumbnail_pending(&path));
}

#[test]
fn cache_clone_shares_state() {
    let cache = ImageCache::with_defaults();
    let cloned = cache.clone();
    let path = PathBuf::from("/tmp/cache-clone/a.jpg");

    cache.insert_thumbnail(path.clone(), dummy_handle());
    assert!(cloned.get_thumbnail(&path).is_some());
}

#[test]
fn clear_resets_everything() {
    let cache = ImageCache::with_defaults();
    let path = PathBuf::from("/tmp/cache-clear/a.jpg");

    cache.insert_full(path.clone(), dummy_cached(10, 10));
    cache.insert_thumbnail(path.clone(), dummy_handle());
    cache.set_pending(path.clone());
    cache.set_thumbnail_pending(path.clone());

    cache.clear();

    assert!(cache.get_full(&path).is_none());
    assert!(cache.get_thumbnail(&path).is_none());
    assert!(!cache.is_pending(&path));
    assert!(!cache.is_thumbnail_pending(&path));
}

#[test]
fn remove_full_leaves_thumbnail() {
    let cache = ImageCache::with_defaults();
    let path = PathBuf::from("/tmp/cache-remove/a.jpg");

    cache.insert_full(path.clone(), dummy_cached(100, 100));
    cache.insert_thumbnail(path.clone(), dummy_handle());

    cache.remove_full(&path);
    assert!(cache.get_full(&path).is_none());
    assert!(cache.get_thumbnail(&path).is_some());
}

#[test]
fn resize_cache_evicts_excess() {
    let cache = ImageCache::new(5, 100);

    for i in 0..5 {
        let path = PathBuf::from(format!("/tmp/cache-resize/{i}.jpg"));
        cache.insert_full(path, dummy_cached(10, 10));
    }

    // Shrink to 2, should evict oldest entries
    cache.resize(2);

    // After resize, at most 2 entries remain
    let mut remaining = 0;
    for i in 0..5 {
        let path = PathBuf::from(format!("/tmp/cache-resize/{i}.jpg"));
        if cache.get_full(&path).is_some() {
            remaining += 1;
        }
    }
    assert!(remaining <= 2);
}
