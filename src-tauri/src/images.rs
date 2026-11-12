//! 이미지 가져오기: 투명 여백을 잘라 `images/`에 PNG로 저장한다.

use image::{imageops::FilterType, ImageReader};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 이보다 큰 이미지는 줄여서 저장한다 (인형은 화면 높이의 일부만 차지한다).
const MAX_HEIGHT: u32 = 1024;
const ALPHA_MIN: u8 = 8;

pub fn new_id() -> String {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{ms:x}{:02x}", SEQ.fetch_add(1, Ordering::Relaxed) & 0xff)
}

/// 불투명 영역의 경계 상자 (x, y, w, h). 전부 투명하면 None.
pub fn opaque_bounds(img: &image::RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, p) in img.enumerate_pixels() {
        if p.0[3] > ALPHA_MIN {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x1 >= x0 && y1 >= y0).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// 인형으로 넣을 수 있는 파일 확장자 (소문자).
pub const EXTENSIONS: &[&str] = &[
    "png", "webp", "gif", "jpg", "jpeg", "jfif", "jpe", "bmp", "dib", "tif", "tiff", "ico", "tga", "qoi", "dds", "pbm", "pgm",
    "ppm", "pnm",
];

pub fn is_supported(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

/// 배경 허용 오차의 범위 (RGB 채널 차이의 최대값). 실제 값은 이미지 테두리의 잡음으로 정한다.
const TOL_MIN: i32 = 6;
const TOL_MAX: i32 = 30;
/// 모서리 색이 서로 이만큼 안이면 단색 배경으로 본다.
const CORNER_TOLERANCE: i32 = 24;

/// 두 색의 차이: RGB 채널 차이 중 가장 큰 값.
fn color_dist(a: [u8; 4], b: [u8; 4]) -> i32 {
    (0..3).map(|i| (a[i] as i32 - b[i] as i32).abs()).max().unwrap_or(0)
}

/// 투명도가 없는 이미지(JPG 등)에서, 테두리와 이어진 단색 배경을 투명하게 만든다.
/// 네 모서리 색이 서로 비슷할 때만 한다 (사진처럼 배경이 단색이 아니면 그대로 둔다).
///
/// 인형의 흰 부분(옷, 신발, 하이라이트)이 흰 배경과 이어져 있어도 먹히지 않도록
///  - 허용 오차를 고정값이 아니라 배경 자체의 잡음(JPG 압축 얼룩)으로 정하고,
///  - 이웃 픽셀과 색이 확 달라지는 곳(인형의 가장자리)에서는 번지기를 멈춘다.
/// 경계 한 줄은 배경색을 걷어 낸 반투명으로 남겨 가장자리에 흰 테가 생기지 않게 한다.
pub fn remove_flat_background(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 || img.pixels().any(|p| p.0[3] < 250) {
        return;
    }
    let corners = [img.get_pixel(0, 0).0, img.get_pixel(w - 1, 0).0, img.get_pixel(0, h - 1).0, img.get_pixel(w - 1, h - 1).0];
    if corners.iter().any(|c| color_dist(*c, corners[0]) > CORNER_TOLERANCE) {
        return;
    }
    let mut avg = [0u32; 3];
    for c in &corners {
        for i in 0..3 {
            avg[i] += c[i] as u32;
        }
    }
    let bg = [(avg[0] / 4) as u8, (avg[1] / 4) as u8, (avg[2] / 4) as u8, 255];

    // 배경의 잡음: 테두리에서 배경색에 가까운 픽셀들의 차이(95번째 백분위).
    let mut border: Vec<i32> = Vec::with_capacity(2 * (w + h) as usize);
    for x in 0..w {
        border.push(color_dist(img.get_pixel(x, 0).0, bg));
        border.push(color_dist(img.get_pixel(x, h - 1).0, bg));
    }
    for y in 0..h {
        border.push(color_dist(img.get_pixel(0, y).0, bg));
        border.push(color_dist(img.get_pixel(w - 1, y).0, bg));
    }
    border.retain(|&d| d <= 40);
    border.sort_unstable();
    let noise = border.get(border.len() * 95 / 100).copied().unwrap_or(10);
    let tol = (noise * 3 / 2 + 4).clamp(TOL_MIN, TOL_MAX);
    // 이웃 픽셀끼리 이보다 크게 다르면 그 사이를 가장자리로 본다.
    let step = (tol / 2).max(4);

    // 테두리에서 시작해 배경색과 이어진 픽셀만 지운다 (인형 안쪽의 같은 색은 남는다).
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    let mut filled = vec![false; (w * h) as usize];
    // (x, y, 들어온 이웃의 색)
    let mut stack: Vec<(u32, u32, Option<[u8; 4]>)> = Vec::new();
    for x in 0..w {
        stack.push((x, 0, None));
        stack.push((x, h - 1, None));
    }
    for y in 0..h {
        stack.push((0, y, None));
        stack.push((w - 1, y, None));
    }
    let mut cleared = 0usize;
    while let Some((x, y, from)) = stack.pop() {
        let i = idx(x, y);
        if filled[i] {
            continue;
        }
        let c = img.get_pixel(x, y).0;
        if color_dist(c, bg) > tol || from.is_some_and(|f| color_dist(c, f) > step) {
            continue;
        }
        filled[i] = true;
        cleared += 1;
        let from = Some(c);
        if x > 0 {
            stack.push((x - 1, y, from));
        }
        if x + 1 < w {
            stack.push((x + 1, y, from));
        }
        if y > 0 {
            stack.push((x, y - 1, from));
        }
        if y + 1 < h {
            stack.push((x, y + 1, from));
        }
    }
    // 거의 다 지워졌다면 배경이 아니라 그림 자체가 단색이었던 것: 그대로 둔다.
    if cleared as f32 > 0.97 * (w * h) as f32 {
        return;
    }

    // 지운 영역에 닿은 가장자리 한 줄: 배경이 섞인 만큼 반투명으로 하고 배경색을 걷어 낸다.
    let span = (0..3).map(|i| (bg[i] as i32).max(255 - bg[i] as i32)).max().unwrap_or(255).max(1) as f32;
    let mut edge: Vec<(u32, u32, [u8; 4])> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if filled[idx(x, y)] {
                continue;
            }
            let touches = (x > 0 && filled[idx(x - 1, y)])
                || (x + 1 < w && filled[idx(x + 1, y)])
                || (y > 0 && filled[idx(x, y - 1)])
                || (y + 1 < h && filled[idx(x, y + 1)]);
            if !touches {
                continue;
            }
            let c = img.get_pixel(x, y).0;
            let a = ((color_dist(c, bg) - tol) as f32 / (span * 0.5)).clamp(0.15, 1.0);
            let mut out = [0u8; 4];
            for i in 0..3 {
                out[i] = ((c[i] as f32 - (1.0 - a) * bg[i] as f32) / a).round().clamp(0.0, 255.0) as u8;
            }
            out[3] = (a * 255.0).round() as u8;
            edge.push((x, y, out));
        }
    }
    for (i, f) in filled.iter().enumerate() {
        if *f {
            img.get_pixel_mut(i as u32 % w, i as u32 / w).0[3] = 0;
        }
    }
    for (x, y, c) in edge {
        img.put_pixel(x, y, image::Rgba(c));
    }
}

/// 이미지를 읽어(PNG/JPG/WebP/BMP/GIF 첫 프레임 등) 여백을 자르고 저장한다. (파일명, 너비/높이)를 돌려준다.
pub fn import(src: &Path, images_dir: &Path, id: &str) -> Result<(String, f32), String> {
    let reader = ImageReader::open(src).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut rgba = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    remove_flat_background(&mut rgba);
    let (x, y, w, h) = opaque_bounds(&rgba).ok_or("완전히 투명한 이미지입니다")?;
    let mut cropped = image::imageops::crop_imm(&rgba, x, y, w, h).to_image();
    if h > MAX_HEIGHT {
        let nw = ((w as f32) * MAX_HEIGHT as f32 / h as f32).round().max(1.0) as u32;
        cropped = image::imageops::resize(&cropped, nw, MAX_HEIGHT, FilterType::Triangle);
    }
    let file = format!("{id}.png");
    std::fs::create_dir_all(images_dir).map_err(|e| e.to_string())?;
    cropped.save(images_dir.join(&file)).map_err(|e| e.to_string())?;
    Ok((file, cropped.width() as f32 / cropped.height() as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn finds_the_opaque_box() {
        let mut img = RgbaImage::new(20, 30);
        for y in 5..25 {
            for x in 4..10 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        img.put_pixel(0, 0, Rgba([0, 0, 0, 3])); // 거의 투명한 잡티는 무시
        assert_eq!(opaque_bounds(&img), Some((4, 5, 6, 20)));
        assert_eq!(opaque_bounds(&RgbaImage::new(4, 4)), None);
    }

    #[test]
    fn flat_background_of_an_opaque_image_is_removed() {
        // 흰 바탕 위 빨간 상자, 상자 안에 흰 점(인형 안쪽의 같은 색은 남아야 한다).
        let mut img = RgbaImage::from_pixel(40, 30, Rgba([250, 250, 248, 255]));
        for y in 5..25 {
            for x in 10..30 {
                img.put_pixel(x, y, Rgba([200, 30, 40, 255]));
            }
        }
        img.put_pixel(20, 15, Rgba([250, 250, 248, 255]));
        remove_flat_background(&mut img);
        assert_eq!(img.get_pixel(0, 0).0[3], 0);
        assert_eq!(img.get_pixel(15, 10).0[3], 255);
        assert_eq!(img.get_pixel(20, 15).0[3], 255);
        assert_eq!(opaque_bounds(&img), Some((10, 5, 20, 20)));
    }

    #[test]
    fn white_parts_touching_a_white_background_are_kept() {
        // JPG처럼 얼룩진 흰 바탕(±2) 위에, 테두리선 없이 바로 붙은 연회색(흰 옷) 영역과 진한 영역.
        let mut img = RgbaImage::new(60, 40);
        let mut seed = 5u32;
        for (x, y, p) in img.enumerate_pixels_mut() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = (seed >> 24) as i32 % 5 - 2;
            let v = (253 + n).clamp(0, 255) as u8;
            *p = Rgba([v, v, v, 255]);
            if (20..40).contains(&x) && (10..30).contains(&y) {
                *p = Rgba([40, 60, 120, 255]);
            }
            if (30..50).contains(&x) && (15..35).contains(&y) {
                *p = Rgba([236, 236, 238, 255]); // 흰 옷: 예전 방식(합 60)이면 지워졌다
            }
        }
        remove_flat_background(&mut img);
        assert_eq!(img.get_pixel(2, 2).0[3], 0);
        assert_eq!(img.get_pixel(45, 25).0[3], 255, "white cloth must stay");
        assert_eq!(img.get_pixel(25, 15).0[3], 255);
        // 가장자리 한 줄은 반투명이지만 남아 있다.
        assert!(img.get_pixel(49, 25).0[3] > 8);
    }

    #[test]
    fn photos_and_transparent_images_are_left_alone() {
        // 모서리 색이 제각각이면 단색 배경이 아니다.
        let mut photo = RgbaImage::from_pixel(10, 10, Rgba([100, 100, 100, 255]));
        photo.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        photo.put_pixel(9, 9, Rgba([0, 0, 255, 255]));
        let before = photo.clone();
        remove_flat_background(&mut photo);
        assert_eq!(photo, before);
        // 이미 투명도가 있는 이미지는 건드리지 않는다.
        let mut png = RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));
        png.put_pixel(5, 5, Rgba([0, 0, 0, 0]));
        let before = png.clone();
        remove_flat_background(&mut png);
        assert_eq!(png, before);
        // 전체가 한 색이면 지우지 않는다.
        let mut flat = RgbaImage::from_pixel(10, 10, Rgba([9, 9, 9, 255]));
        remove_flat_background(&mut flat);
        assert!(flat.pixels().all(|p| p.0[3] == 255));
    }

    #[test]
    fn imports_jpg_and_bmp() {
        let dir = std::env::temp_dir().join(format!("plushbounce-test-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut img = image::RgbImage::from_pixel(60, 60, image::Rgb([255, 255, 255]));
        for y in 10..50 {
            for x in 20..40 {
                img.put_pixel(x, y, image::Rgb([30, 120, 220]));
            }
        }
        for ext in ["jpg", "bmp", "tiff"] {
            let src = dir.join(format!("doll.{ext}"));
            img.save(&src).unwrap();
            assert!(is_supported(src.to_str().unwrap()));
            let (_, aspect) = import(&src, &dir, &format!("x{ext}")).unwrap();
            // JPG는 손실 압축이라 가장자리가 1~2픽셀 번질 수 있다.
            assert!((aspect - 0.5).abs() < 0.08, "{ext}: aspect {aspect}");
        }
        assert!(!is_supported("notes.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_trims_and_reports_aspect() {
        let dir = std::env::temp_dir().join(format!("plushbounce-test-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut img = RgbaImage::new(100, 100);
        for y in 10..90 {
            for x in 30..70 {
                img.put_pixel(x, y, Rgba([10, 200, 30, 255]));
            }
        }
        let src = dir.join("src.png");
        img.save(&src).unwrap();
        let (file, aspect) = import(&src, &dir, "abc").unwrap();
        assert_eq!(file, "abc.png");
        assert!((aspect - 0.5).abs() < 1e-6);
        assert_eq!(image::open(dir.join(file)).unwrap().to_rgba8().dimensions(), (40, 80));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
