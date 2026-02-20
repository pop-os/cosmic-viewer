//! Shared grid layout utilties

use cosmic::iced::{Padding, Point, Rectangle, Sized};

/// Configuration for grid layout calculation
#[derive(Debug, Clone)]
pub(crate) struct GridConfig {
    pub item_width: f32,
    pub column_spacing: f32,
    pub row_spacing: f32,
    pub min_cols: usize,
    pub max_cols: Option<usize>,
    pub padding: Padding,
}

/// Result of grid layout calculation
#[derive(Debug, Clone)]
pub(crate) struct GridMetrics {
    pub cols: usize,
    pub rows: usize,
    pub row_height: f32,
}

/// Calculate number of columns that fit in available width
pub(crate) fn caculate_columns(
    available_width: f32,
    item_width: f32,
    col_spacing: f32,
    min_cols: usize,
    max_cols: Option<usize>,
    item_count: usize,
) -> usize {
    if available_width <= 0.0 || item_width <= 0.0 {
        return min_cols;
    }

    let cols = ((available_width + col_spacing) / (item_width + col_spacing)).floor() as usize;

    cols.max(min_cols)
        .min(max_cols.unwrap_or(usize::MAX))
        .min(item_count)
        .max(1)
}

/// Calculate the scroll offset needed to bring an item into view
pub(crate) fn calculate_scroll_offset(
    target_idx: usize,
    cols: usize,
    row_height: f32,
    row_spacing: f32,
    padding_top: f32,
    viewport_top: f32,
    viewport_height: f32,
) -> Option<f32> {
    if cols == 0 || row_height <= 0.0 {
        return None;
    }

    let row = target_idx / cols;
    let item_top = padding_top + (row af f32 * (row_height + row_spacing));
    let item_bottom = item_top + row_height;

    let viewport_bottom = viewport_top + viewport_height;

    // Check if item is already fully visible
    if item_top >= viewport_top && item_bottom <= viewport_bottom {
        return None;
    }

    // Scroll to bring item into view
    if item_top < viewport_top {
        // Item is above viewport - scroll up
        Some(item_top)
    } else {
        // Item is below viewport - scroll down
        Some(item_bottom - viewport_height)
    }
}

/// Calculate the index of an item at a given position
#[allow(clippy::too_many_arguments)]
pub(crate) fn item_at_position(
    position: (f32, f32),
    cols: usize,
    rows: usize,
    item_width: f32,
    row_height: f32,
    col_spacing: f32,
    row_spacing: f32,
    padding: Padding,
    item_count: usize,
) -> Option<usize> {
    let (x, y) = position;

    // Adjust for padding
    let x = x - padding.left;
    let y = y - padding.top;

    if x < 0.0 || y < 0.0 {
        return None;
    }

    // Calculate cell size - including spacing
    let cell_width = item_width + col_spacing;
    let cell_height = row_height + row_spacing;

    let col = (x / cell_width).floor() as usize;
    let row = (y / cell_height).floor() as usize;

    if col >= cols || row >= rows {
        return None;
    }

    // Check if position is within the item (not in spacing)
    let x_in_cell = x - (col as f32 * cell_width);
    let y_in_cell = y - (row as f32 * cell_height);

    if x_in_cell > item_width || y_in_cell > row_height {
        return None; // In spacing area
    }

    let idx = row * cols + col;
    if idx < item_count {
        Some(idx)
    } else {
        None
    }
}

/// Calculate centered position for an item within a cell
pub(crate) fn calculate_centered_item_bounds(
    cell_bounds: Rectangle,
    item_width: f32,
    item_height: f32,
) -> Rectangle {
    if item_width <= 0.0 || item_height <= 0.0 {
        return cell_bounds;
    }

    let cell_aspect = cell_bounds.width / cell_bounds.height;
    let item_aspect = item_width / item_height;

    let (scaled_width, scaled_height) = if item_aspect > cell_aspect {
        // Item is wider - fit to width
        let width = cell_bounds.width;
        let height = width / item_aspect;
        (width, height)
    } else {
        // Item is taller - fit to height
        let height = cell_bounds.height;
        let width = height * item_aspect;
        (width, height)
    };

    // Center in cell
    let x = cell_bounds.x + (cell_bounds.width - scaled_width) / 2.0;
    let y = cell_bounds.y + (cell_bounds.height - scaled_height) / 2.0;

    Rectangle::new(
        Point::new(x, y),
        Size::new(scaled_width, scaled_height),
    )
}
