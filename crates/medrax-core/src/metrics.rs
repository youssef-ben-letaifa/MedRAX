use std::collections::HashMap;
use std::path::Path;
use anyhow::{Context, Result};
use image::ImageReader;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganMetrics {
    pub area_pixels: u64,
    pub area_cm2: f64,
    pub centroid: (f64, f64), // (y, x)
    pub bbox: (u32, u32, u32, u32), // (min_y, min_x, max_y, max_x)
    pub width: u32,
    pub height: u32,
    pub aspect_ratio: f64,
    pub relative_position: HashMap<String, f64>,
    pub mean_intensity: f64,
    pub std_intensity: f64,
    pub confidence_score: f64,
}

/// Compute metrics from mask and image buffers
pub fn compute_metrics_from_buffers(
    mask_pixels: &[u8],
    img_pixels: &[u8],
    width: u32,
    height: u32,
    pixel_spacing_mm: f64,
    confidence_score: f64,
) -> Option<OrganMetrics> {
    if mask_pixels.is_empty() || width == 0 || height == 0 {
        return None;
    }

    let mut area_pixels: u64 = 0;
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut sum_x: f64 = 0.0;
    let mut sum_y: f64 = 0.0;
    let mut sum_intensity: f64 = 0.0;
    let mut sum_sq_intensity: f64 = 0.0;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if idx < mask_pixels.len() && mask_pixels[idx] > 0 {
                area_pixels += 1;
                if x < min_x { min_x = x; }
                if x > max_x { max_x = x; }
                if y < min_y { min_y = y; }
                if y > max_y { max_y = y; }

                sum_x += x as f64;
                sum_y += y as f64;

                let intensity = if idx < img_pixels.len() {
                    img_pixels[idx] as f64
                } else {
                    0.0
                };
                sum_intensity += intensity;
                sum_sq_intensity += intensity * intensity;
            }
        }
    }

    if area_pixels == 0 {
        return None;
    }

    let area_f = area_pixels as f64;
    let cy = sum_y / area_f;
    let cx = sum_x / area_f;

    let spacing_cm = pixel_spacing_mm / 10.0;
    let area_cm2 = area_f * (spacing_cm * spacing_cm);

    let bbox_width = if max_x >= min_x { max_x - min_x + 1 } else { 0 };
    let bbox_height = if max_y >= min_y { max_y - min_y + 1 } else { 0 };
    let aspect_ratio = bbox_height as f64 / (bbox_width.max(1) as f64);

    let img_h = height as f64;
    let img_w = width as f64;
    let top_rel = cy / img_h;
    let left_rel = cx / img_w;
    let center_dist = ((top_rel - 0.5).powi(2) + (left_rel - 0.5).powi(2)).sqrt();

    let mut relative_pos = HashMap::new();
    relative_pos.insert("top".to_string(), top_rel);
    relative_pos.insert("left".to_string(), left_rel);
    relative_pos.insert("center_dist".to_string(), center_dist);

    let mean_intensity = sum_intensity / area_f;
    let variance = (sum_sq_intensity / area_f) - (mean_intensity * mean_intensity);
    let std_intensity = if variance > 0.0 { variance.sqrt() } else { 0.0 };

    Some(OrganMetrics {
        area_pixels,
        area_cm2,
        centroid: (cy, cx),
        bbox: (min_y, min_x, max_y, max_x),
        width: bbox_width,
        height: bbox_height,
        aspect_ratio,
        relative_position: relative_pos,
        mean_intensity,
        std_intensity,
        confidence_score,
    })
}

/// Compute metrics from image and mask files on disk
pub fn compute_metrics_from_files<P: AsRef<Path>, Q: AsRef<Path>>(
    mask_path: P,
    image_path: Q,
    pixel_spacing_mm: f64,
    confidence_score: f64,
) -> Result<Option<OrganMetrics>> {
    let mask_img = ImageReader::open(mask_path.as_ref())
        .with_context(|| format!("Could not open mask image: {:?}", mask_path.as_ref()))?
        .decode()?
        .to_luma8();

    let orig_img = ImageReader::open(image_path.as_ref())
        .with_context(|| format!("Could not open original image: {:?}", image_path.as_ref()))?
        .decode()?
        .to_luma8();

    let (w, h) = mask_img.dimensions();
    let metrics = compute_metrics_from_buffers(
        mask_img.as_raw(),
        orig_img.as_raw(),
        w,
        h,
        pixel_spacing_mm,
        confidence_score,
    );

    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_metrics_basic() {
        // 4x4 image with a 2x2 square mask at (1,1) to (2,2)
        let mask = vec![
            0, 0, 0, 0,
            0, 1, 1, 0,
            0, 1, 1, 0,
            0, 0, 0, 0,
        ];
        let img = vec![
            10, 10, 10, 10,
            10, 100, 100, 10,
            10, 200, 200, 10,
            10, 10, 10, 10,
        ];

        let metrics = compute_metrics_from_buffers(&mask, &img, 4, 4, 1.0, 0.95).unwrap();
        assert_eq!(metrics.area_pixels, 4);
        assert_eq!(metrics.bbox, (1, 1, 2, 2));
        assert_eq!(metrics.width, 2);
        assert_eq!(metrics.height, 2);
        assert_eq!(metrics.centroid, (1.5, 1.5));
        assert_eq!(metrics.mean_intensity, 150.0);
        assert_eq!(metrics.confidence_score, 0.95);
    }
}
