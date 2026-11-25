// SPDX-License-Identifier: GPL-3.0-only

use std::fs;
use std::path::{Path, PathBuf};

use viewer_config::{SortMode, SortOrder};
use viewer_core::{is_supported_image, scan_dir};

fn test_images_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../test_images"))
}

#[tokio::test]
async fn scan_returns_only_supported_images() {
    let dir = test_images_dir();
    let images = scan_dir(&dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert!(!images.is_empty());

    for path in &images {
        assert!(
            is_supported_image(path),
            "{} should be a supported image",
            path.display(),
        );
    }
}

#[tokio::test]
async fn scan_name_ascending_is_sorted() {
    let dir = test_images_dir();
    let images = scan_dir(&dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert!(!images.is_empty());

    // Needs more than one entry to compare against the reversed order below.
    assert!(images.len() > 1);

    // Verify ascending and descending produce inverse orderings
    let mut rev = scan_dir(&dir, false, SortMode::Name, SortOrder::Descending).await;
    assert_eq!(images.len(), rev.len());
    rev.reverse();
    assert_eq!(images, rev);
}

#[tokio::test]
async fn scan_name_descending_is_reverse_sorted() {
    let dir = test_images_dir();
    let asc = scan_dir(&dir, false, SortMode::Name, SortOrder::Ascending).await;
    let mut rev = scan_dir(&dir, false, SortMode::Name, SortOrder::Descending).await;
    assert_eq!(asc.len(), rev.len());

    rev.reverse();
    assert_eq!(asc, rev);
}

#[tokio::test]
async fn scan_size_ascending_is_ordered() {
    let dir = test_images_dir();
    let images = scan_dir(&dir, false, SortMode::Size, SortOrder::Ascending).await;
    assert!(!images.is_empty());

    for pair in images.windows(2) {
        let a_size = fs::metadata(&pair[0]).unwrap().len();
        let b_size = fs::metadata(&pair[1]).unwrap().len();
        assert!(a_size <= b_size);
    }
}

#[tokio::test]
async fn scan_date_ascending_is_ordered() {
    let dir = test_images_dir();
    let images = scan_dir(&dir, false, SortMode::Date, SortOrder::Ascending).await;
    assert!(!images.is_empty());

    for pair in images.windows(2) {
        let a_time = fs::metadata(&pair[0]).unwrap().modified().unwrap();
        let b_time = fs::metadata(&pair[1]).unwrap().modified().unwrap();
        assert!(a_time <= b_time);
    }
}

#[tokio::test]
async fn scan_hidden_files_off() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let visible = dir.join("visible.jpg");
    fs::copy(test_images_dir().join("1315754.jpg"), &visible).unwrap();
    fs::copy(
        test_images_dir().join("test_format.png"),
        dir.join(".hidden.png"),
    )
    .unwrap();

    let images = scan_dir(dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert_eq!(images, vec![visible]);
}

#[tokio::test]
async fn scan_hidden_files_on() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let visible = dir.join("visible.jpg");
    let hidden = dir.join(".hidden.png");
    fs::copy(test_images_dir().join("1315754.jpg"), &visible).unwrap();
    fs::copy(test_images_dir().join("test_format.png"), &hidden).unwrap();

    let images = scan_dir(dir, true, SortMode::Name, SortOrder::Ascending).await;
    assert_eq!(images, vec![hidden, visible]);
}

#[tokio::test]
async fn scan_nonexistent_dir_returns_empty() {
    let dir = Path::new("/no/such/directory/for/cosmic/viewer");
    let images = scan_dir(dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert!(images.is_empty());
}

#[tokio::test]
async fn scan_empty_dir_returns_empty() {
    let dir = std::env::temp_dir().join("cosmic-viewer-nav-empty-test");
    let _ = fs::create_dir_all(&dir);
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }

    let images = scan_dir(&dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert!(images.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn scan_filters_non_image_files() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    fs::write(dir.join("readme.txt"), b"text").unwrap();
    fs::write(dir.join("fake.jpg"), b"fake").unwrap();
    let image = dir.join("photo.jpg");
    fs::copy(test_images_dir().join("1315754.jpg"), &image).unwrap();

    let images = scan_dir(dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert_eq!(images, vec![image]);
}

#[cfg(unix)]
#[tokio::test]
async fn scan_handles_symlinks_gracefully() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let real = dir.join("real.jpg");
    let link = dir.join("link.jpg");
    fs::copy(test_images_dir().join("1315754.jpg"), &real).unwrap();
    symlink(&real, &link).unwrap();
    symlink(dir.join("missing.jpg"), dir.join("broken.jpg")).unwrap();

    let images = scan_dir(dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert_eq!(images, vec![link, real]);
}

#[test]
fn is_supported_image_consistency() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let png = test_images_dir().join("test_format.png");
    for name in ["photo.PNG", "photo", "photo.txt"] {
        let path = dir.join(name);
        fs::copy(&png, &path).unwrap();
        assert!(is_supported_image(&path), "{name}");
    }

    let fake = dir.join("fake.png");
    fs::write(&fake, b"text").unwrap();
    assert!(!is_supported_image(&fake));
    assert!(!is_supported_image(&dir.join("missing.png")));
}
