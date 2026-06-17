use std::path::PathBuf;
use viewer_core::{load_image, load_thumbnail};

fn test_images_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../test_images"))
}

#[tokio::test]
async fn load_jpeg_has_valid_dimensions() {
    let path = test_images_dir().join("1315754.jpg");
    let loaded = load_image(path.clone()).await.expect("should load JPEG");
    assert!(loaded.width > 0);
    assert!(loaded.height > 0);
    assert_eq!(loaded.path, path);
}

#[tokio::test]
async fn load_png_has_valid_dimensions() {
    let path = test_images_dir().join("test_format.png");
    let loaded = load_image(path.clone()).await.expect("should load PNG");
    assert!(loaded.width > 0);
    assert!(loaded.height > 0);
    assert_eq!(loaded.path, path);
}

#[tokio::test]
async fn load_webp_has_valid_dimensions() {
    let path = test_images_dir().join("test_format.webp");
    let loaded = load_image(path.clone()).await.expect("should load WebP");
    assert!(loaded.width > 0);
    assert!(loaded.height > 0);
}

#[tokio::test]
async fn load_nonexistent_file_returns_error() {
    let path = test_images_dir().join("no_such_file_at_all.jpg");
    assert!(load_image(path).await.is_err());
}

#[tokio::test]
async fn load_non_image_file_returns_error() {
    let tmp = std::env::temp_dir().join("cosmic-viewer-test-not-image.txt");
    std::fs::write(&tmp, b"this is not an image").unwrap();
    let result = load_image(tmp.clone()).await;
    assert!(result.is_err());
    let _ = std::fs::remove_file(&tmp);
}

#[tokio::test]
async fn thumbnail_smaller_than_original() {
    let path = test_images_dir().join("1315754.jpg");
    let full = load_image(path.clone()).await.expect("full load");
    let thumb = load_thumbnail(path, 128).await.expect("thumb load");

    let full_pixels = full.width as u64 * full.height as u64;
    let thumb_pixels = thumb.width as u64 * thumb.height as u64;
    assert!(thumb_pixels < full_pixels);
    assert!(thumb.width <= 128);
    assert!(thumb.height <= 128);
}

#[tokio::test]
async fn load_same_image_twice_deterministic() {
    let path = test_images_dir().join("1315754.jpg");
    let a = load_image(path.clone()).await.expect("first load");
    let b = load_image(path).await.expect("second load");
    assert_eq!(a.width, b.width);
    assert_eq!(a.height, b.height);
}

#[tokio::test]
async fn load_bmp_has_valid_dimensions() {
    let path = test_images_dir().join("test_format.bmp");
    let loaded = load_image(path).await.expect("should load BMP");
    assert!(loaded.width > 0 && loaded.height > 0);
}

#[tokio::test]
async fn load_gif_has_valid_dimensions() {
    let path = test_images_dir().join("test_format.gif");
    let loaded = load_image(path).await.expect("should load GIF");
    assert!(loaded.width > 0 && loaded.height > 0);
}

#[tokio::test]
async fn load_tiff_has_valid_dimensions() {
    let path = test_images_dir().join("test_format.tiff");
    let loaded = load_image(path).await.expect("should load TIFF");
    assert!(loaded.width > 0 && loaded.height > 0);
}

#[tokio::test]
async fn load_ico_has_valid_dimensions() {
    let path = test_images_dir().join("test_format.ico");
    let loaded = load_image(path).await.expect("should load ICO");
    assert!(loaded.width > 0 && loaded.height > 0);
}

#[tokio::test]
async fn thumbnail_at_multiple_sizes() {
    let path = test_images_dir().join("1315754.jpg");
    for max in [64, 128, 256] {
        let thumb = load_thumbnail(path.clone(), max)
            .await
            .unwrap_or_else(|e| panic!("thumb at {max} failed: {e}"));
        assert!(thumb.width <= max && thumb.height <= max);
        assert!(thumb.width > 0 && thumb.height > 0);
    }
}

#[tokio::test]
async fn thumbnail_preserves_aspect_ratio() {
    let path = test_images_dir().join("1315754.jpg");
    let full = load_image(path.clone()).await.expect("full");
    let thumb = load_thumbnail(path, 256).await.expect("thumb");

    let full_ratio = full.width as f64 / full.height as f64;
    let thumb_ratio = thumb.width as f64 / thumb.height as f64;
    assert!(
        (full_ratio - thumb_ratio).abs() < 0.05,
        "aspect ratio diverged: full={full_ratio:.3}, thumb={thumb_ratio:.3}",
    );
}
