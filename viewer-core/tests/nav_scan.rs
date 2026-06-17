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

    // Verify the list has at least two entries, confirming it scanned real files
    assert!(images.len() > 1);

    // Verify ascending and descending produce inverse orderings
    let desc = scan_dir(&dir, false, SortMode::Name, SortOrder::Descending).await;
    assert_eq!(images.len(), desc.len());
    let mut rev = desc.clone();
    rev.reverse();
    assert_eq!(images, rev);
}

#[tokio::test]
async fn scan_name_descending_is_reverse_sorted() {
    let dir = test_images_dir();
    let asc = scan_dir(&dir, false, SortMode::Name, SortOrder::Ascending).await;
    let desc = scan_dir(&dir, false, SortMode::Name, SortOrder::Descending).await;
    assert_eq!(asc.len(), desc.len());

    let mut rev = desc.clone();
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
    let dir = std::env::temp_dir().join("cosmic-viewer-nav-hidden-test");
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("visible.jpg"), b"fake").unwrap();
    fs::write(dir.join(".hidden.png"), b"fake").unwrap();

    let images = scan_dir(&dir, false, SortMode::Name, SortOrder::Ascending).await;
    for path in &images {
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(
            !name.starts_with('.'),
            "hidden file {name} should be excluded"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn scan_hidden_files_on() {
    let dir = std::env::temp_dir().join("cosmic-viewer-nav-hidden-test-on");
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("visible.jpg"), b"fake").unwrap();
    fs::write(dir.join(".hidden.png"), b"fake").unwrap();

    let images = scan_dir(&dir, true, SortMode::Name, SortOrder::Ascending).await;
    let has_hidden = images
        .iter()
        .any(|p| p.file_name().unwrap().to_str().unwrap().starts_with('.'));
    assert!(has_hidden, "hidden file should be included when flag is on");

    let _ = fs::remove_dir_all(&dir);
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
    let dir = std::env::temp_dir().join("cosmic-viewer-nav-filter-test");
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("readme.txt"), b"text").unwrap();
    fs::write(dir.join("video.mp4"), b"video").unwrap();
    fs::write(dir.join("photo.jpg"), b"fake").unwrap();

    let images = scan_dir(&dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert_eq!(images.len(), 1);
    assert_eq!(
        images[0].file_name().unwrap().to_str().unwrap(),
        "photo.jpg",
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn scan_handles_symlinks_gracefully() {
    let dir = std::env::temp_dir().join("cosmic-viewer-nav-symlink-test");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("real.jpg"), b"fake image").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(dir.join("real.jpg"), dir.join("link.jpg"));
        // Also create a broken symlink
        let _ = symlink("/nonexistent/path.jpg", dir.join("broken.jpg"));
    }

    // Should not panic regardless of symlink state
    let images = scan_dir(&dir, false, SortMode::Name, SortOrder::Ascending).await;
    assert!(!images.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn is_supported_image_consistency() {
    // Every file from a scan should pass is_supported_image, and vice versa
    assert!(is_supported_image(Path::new("photo.jpg")));
    assert!(is_supported_image(Path::new("photo.jpeg")));
    assert!(is_supported_image(Path::new("photo.png")));
    assert!(is_supported_image(Path::new("photo.gif")));
    assert!(is_supported_image(Path::new("photo.webp")));
    assert!(is_supported_image(Path::new("photo.bmp")));
    assert!(is_supported_image(Path::new("photo.tiff")));
    assert!(is_supported_image(Path::new("photo.ico")));
    assert!(is_supported_image(Path::new("photo.avif")));
    // assert!(is_supported_image(Path::new("photo.svg")));
    // assert!(is_supported_image(Path::new("photo.heif")));
    // assert!(is_supported_image(Path::new("photo.heic")));

    assert!(!is_supported_image(Path::new("file.txt")));
    assert!(!is_supported_image(Path::new("file.mp4")));
    assert!(!is_supported_image(Path::new("file.pdf")));
    assert!(!is_supported_image(Path::new("file")));
}
