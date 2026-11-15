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
/// 테두리 픽셀을 한 배경색으로 묶는 거리.
const GROUP: i32 = 20;
/// 테두리의 이만큼 이상이 한 색이어야 단색 배경으로 본다 (나머지는 테두리에 닿은 인형).
const BORDER_SHARE: f32 = 0.45;
/// 체크무늬(가짜 투명 배경): 두 회색이 테두리의 이만큼 이상을 덮고, 테두리를 따라 이만큼 넘게 번갈아 나와야 한다.
const CHECKER_SHARE: f32 = 0.8;
const CHECKER_SWITCHES: usize = 12;
/// 인형 안쪽에 갇힌 배경(팔과 몸 사이 등)은 이미지 넓이의 1/HOLE_DIV 이상일 때만 지운다 (눈 흰자 같은 작은 곳은 남긴다).
const HOLE_DIV: u32 = 300;

/// 두 색의 차이: RGB 채널 차이 중 가장 큰 값.
fn color_dist(a: [u8; 4], b: [u8; 4]) -> i32 {
    (0..3).map(|i| (a[i] as i32 - b[i] as i32).abs()).max().unwrap_or(0)
}

/// 채도가 낮은(회색 계열) 색인가.
fn neutral(c: [u8; 4]) -> bool {
    let (lo, hi) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    hi - lo <= 20
}

/// 이미지 테두리를 시계 방향으로 한 바퀴 돈 픽셀들.
fn border_ring(img: &image::RgbaImage) -> Vec<[u8; 4]> {
    let (w, h) = img.dimensions();
    let mut v = Vec::with_capacity(2 * (w + h) as usize);
    v.extend((0..w).map(|x| img.get_pixel(x, 0).0));
    v.extend((1..h).map(|y| img.get_pixel(w - 1, y).0));
    v.extend((0..w - 1).rev().map(|x| img.get_pixel(x, h - 1).0));
    v.extend((1..h - 1).rev().map(|y| img.get_pixel(0, y).0));
    v
}

/// 가장 흔한 색 (그 색 근처 픽셀들의 평균)과, 그 색에 가까운 픽셀 수.
fn dominant(pixels: &[[u8; 4]]) -> Option<([u8; 4], usize)> {
    if pixels.is_empty() {
        return None;
    }
    let key = |p: &[u8; 4]| ((p[0] as usize >> 4) << 8) | ((p[1] as usize >> 4) << 4) | (p[2] as usize >> 4);
    let mut hist = vec![0u32; 4096];
    for p in pixels {
        hist[key(p)] += 1;
    }
    let (k, _) = hist.iter().enumerate().max_by_key(|(k, n)| (**n, std::cmp::Reverse(*k)))?;
    let center = [((k >> 8) as u8) * 16 + 8, (((k >> 4) & 15) as u8) * 16 + 8, ((k & 15) as u8) * 16 + 8, 255];
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for p in pixels.iter().filter(|p| color_dist(**p, center) <= GROUP) {
        for i in 0..3 {
            sum[i] += p[i] as u64;
        }
        n += 1;
    }
    let n = n.max(1);
    let c = [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, 255];
    Some((c, pixels.iter().filter(|p| color_dist(**p, c) <= GROUP).count()))
}

/// 배경색 (단색이면 1개, 체크무늬면 2개). 단색 배경이 아니면 None.
fn background_palette(ring: &[[u8; 4]]) -> Option<Vec<[u8; 4]>> {
    let total = ring.len() as f32;
    let (c1, n1) = dominant(ring)?;
    let rest: Vec<[u8; 4]> = ring.iter().copied().filter(|p| color_dist(*p, c1) > GROUP).collect();
    if let Some((c2, n2)) = dominant(&rest) {
        if neutral(c1) && neutral(c2) && (n1 + n2) as f32 >= CHECKER_SHARE * total && n2 as f32 >= 0.15 * total {
            // 테두리를 따라 두 색이 몇 번 번갈아 나오는가 (회색 인형이 테두리에 닿은 것과 체크무늬를 가른다).
            let class = |p: &[u8; 4]| {
                if color_dist(*p, c1) <= GROUP {
                    Some(0)
                } else if color_dist(*p, c2) <= GROUP {
                    Some(1)
                } else {
                    None
                }
            };
            let mut prev = None;
            let mut switches = 0;
            for c in ring.iter().filter_map(class) {
                if prev.is_some_and(|p| p != c) {
                    switches += 1;
                }
                prev = Some(c);
            }
            if switches > CHECKER_SWITCHES {
                return Some(vec![c1, c2]);
            }
        }
    }
    (n1 as f32 >= BORDER_SHARE * total).then(|| vec![c1])
}

/// 투명도가 없는 이미지(JPG 등)에서 배경을 투명하게 만든다.
///
///  - 배경색은 테두리에서 가장 흔한 색으로 정한다 (인형이 모서리·바닥에 닿아 있어도 된다).
///    검색해서 받은 "가짜 투명" 체크무늬 배경도 알아본다. 테두리가 제각각(사진)이면 그대로 둔다.
///  - 허용 오차는 고정값이 아니라 배경 자체의 잡음(JPG 압축 얼룩)으로 정하고,
///    이웃 픽셀과 색이 확 달라지는 곳(인형의 가장자리)에서는 번지기를 멈춘다.
///    그래서 인형의 흰 부분(옷, 신발, 하이라이트)이 흰 배경과 이어져 있어도 먹히지 않는다.
///  - 인형 안쪽에 갇힌 넓고 평평한 배경(팔과 몸 사이)도 지운다.
///  - 경계 한 줄은 배경색을 걷어 낸 반투명으로 남겨 흰 테가 생기지 않게 하고,
///    배경에 남은 작은 얼룩은 지워서 여백이 깔끔하게 잘리게 한다.
pub fn remove_flat_background(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 || img.pixels().any(|p| p.0[3] < 250) {
        return;
    }
    let ring = border_ring(img);
    let Some(palette) = background_palette(&ring) else { return };
    // 가장 가까운 배경색 (차이, 번호).
    let nearest = |c: [u8; 4]| palette.iter().enumerate().map(|(i, b)| (color_dist(c, *b), i)).min().unwrap_or((255, 0));

    // 배경의 잡음: 테두리에서 배경색에 가까운 픽셀들의 차이(95번째 백분위).
    let mut border: Vec<i32> = ring.iter().map(|p| nearest(*p).0).filter(|&d| d <= 40).collect();
    border.sort_unstable();
    let noise = border.get(border.len() * 95 / 100).copied().unwrap_or(10);
    let tol = (noise * 3 / 2 + 4).clamp(TOL_MIN, TOL_MAX);
    // 이웃 픽셀끼리 이보다 크게 다르면 그 사이를 가장자리로 본다.
    let step = (tol / 2).max(4);
    // 배경과 거의 똑같은 색 (갇힌 배경 찾기, 체크무늬의 다른 칸으로 건너가기).
    let tight = (noise + 4).clamp(4, tol);

    let idx = |x: u32, y: u32| (y * w + x) as usize;
    // 배경 칠하기: seeds에서 시작해 배경색과 이어진 픽셀을 mark에 칠하고, 칠한 픽셀들을 돌려준다.
    // 같은 배경색 안에서는 이웃과 step 넘게 달라지면 멈추고, 체크무늬의 다른 칸으로는 tight 안일 때만 넘어간다.
    let flood = |img: &image::RgbaImage, mark: &mut [bool], seeds: Vec<(u32, u32)>, limit: i32| -> Vec<usize> {
        let mut done = Vec::new();
        let mut stack: Vec<(u32, u32, Option<([u8; 4], usize)>)> = seeds.into_iter().map(|(x, y)| (x, y, None)).collect();
        while let Some((x, y, from)) = stack.pop() {
            let i = idx(x, y);
            if mark[i] {
                continue;
            }
            let c = img.get_pixel(x, y).0;
            let (d, k) = nearest(c);
            if d > limit {
                continue;
            }
            if let Some((f, fk)) = from {
                let blocked = if fk == k { color_dist(c, f) > step } else { d > tight };
                if blocked {
                    continue;
                }
            }
            mark[i] = true;
            done.push(i);
            let from = Some((c, k));
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
        done
    };

    // 1) 테두리와 이어진 배경.
    let mut filled = vec![false; (w * h) as usize];
    let mut seeds = Vec::with_capacity(2 * (w + h) as usize);
    for x in 0..w {
        seeds.extend([(x, 0), (x, h - 1)]);
    }
    for y in 0..h {
        seeds.extend([(0, y), (w - 1, y)]);
    }
    let mut cleared = flood(img, &mut filled, seeds, tol).len();

    // 2) 인형 안쪽에 갇힌 배경: 배경과 거의 같은 색이고, 고르게 평평하고, 넓은 곳만.
    let min_hole = ((w * h) / HOLE_DIV).max(24) as usize;
    let mut seen = filled.clone();
    for y in 0..h {
        for x in 0..w {
            if seen[idx(x, y)] || nearest(img.get_pixel(x, y).0).0 > tight {
                continue;
            }
            let region = flood(img, &mut seen, vec![(x, y)], tight);
            if region.len() < min_hole {
                continue;
            }
            let mut dists: Vec<i32> = region.iter().map(|&j| nearest(img.get_pixel(j as u32 % w, j as u32 / w).0).0).collect();
            dists.sort_unstable();
            if dists[dists.len() * 9 / 10] <= noise + 2 {
                cleared += region.len();
                for j in region {
                    filled[j] = true;
                }
            }
        }
    }

    // 거의 다 지워졌다면 배경이 아니라 그림 자체가 단색이었던 것: 그대로 둔다.
    if cleared as f32 > 0.97 * (w * h) as f32 {
        return;
    }

    let edge = unmix_edges(img, &filled, &palette, tol);
    for (i, f) in filled.iter().enumerate() {
        if *f {
            img.get_pixel_mut(i as u32 % w, i as u32 / w).0[3] = 0;
        }
    }
    for (x, y, c) in edge {
        img.put_pixel(x, y, image::Rgba(c));
    }
    drop_specks(img);
}

/// 지운 배경에 닿은 가장자리 띠(몇 픽셀)에서 배경색을 걷어 낸다. 고칠 (x, y, 새 색)들을 돌려준다.
///
/// 가장자리 픽셀은 인형 색 F와 배경색 B가 섞인 것(c = a·F + (1−a)·B)으로 본다. F는 띠 바로 안쪽의
/// 인형 픽셀들의 평균으로 어림하고, 섞인 비율 a를 불투명도로 삼아 배경을 뺀 색을 남긴다.
/// 그래서 JPG의 부드러운 경계가 흰 테 없이 반투명으로 남는다.
/// 인형 색이 배경과 거의 같은 곳(흰 배경 위의 흰 옷)은 비율을 알 수 없으므로 그대로 둔다.
fn unmix_edges(img: &image::RgbaImage, filled: &[bool], palette: &[[u8; 4]], tol: i32) -> Vec<(u32, u32, [u8; 4])> {
    let (w, h) = img.dimensions();
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    // 띠 두께: 큰 이미지일수록 경계가 넓게 번져 있다.
    let band = (w.max(h) / 300).clamp(3, 5) as u8;
    let reach = band as i64 + 2;
    // 지운 곳에서의 거리 (지운 곳 = 0, band+2 너머 = MAX).
    let mut depth = vec![u8::MAX; filled.len()];
    let mut queue = std::collections::VecDeque::new();
    for (i, f) in filled.iter().enumerate() {
        if *f {
            depth[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let d = depth[i];
        if d as i64 >= reach {
            continue;
        }
        let (x, y) = (i as u32 % w, i as u32 / w);
        let mut visit = |j: usize| {
            if depth[j] == u8::MAX {
                depth[j] = d + 1;
                queue.push_back(j);
            }
        };
        if x > 0 {
            visit(i - 1);
        }
        if x + 1 < w {
            visit(i + 1);
        }
        if y > 0 {
            visit(i - w as usize);
        }
        if y + 1 < h {
            visit(i + w as usize);
        }
    }

    let nearest_bg = |c: [u8; 4]| *palette.iter().min_by_key(|b| color_dist(c, **b)).unwrap_or(&palette[0]);
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let dd = depth[idx(x, y)];
            if dd == 0 || dd > band {
                continue;
            }
            let c = img.get_pixel(x, y).0;
            let bg = nearest_bg(c);
            // 안쪽 인형 색 F: 띠보다 깊은 주변 픽셀들의 평균.
            let (mut sum, mut n) = ([0f32; 3], 0f32);
            for yy in (y as i64 - reach).max(0)..(y as i64 + reach + 1).min(h as i64) {
                for xx in (x as i64 - reach).max(0)..(x as i64 + reach + 1).min(w as i64) {
                    if depth[idx(xx as u32, yy as u32)] > band {
                        let p = img.get_pixel(xx as u32, yy as u32).0;
                        for i in 0..3 {
                            sum[i] += p[i] as f32;
                        }
                        n += 1.0;
                    }
                }
            }
            let a = if n > 0.0 {
                let f: [f32; 3] = std::array::from_fn(|i| sum[i] / n);
                let fb: [f32; 3] = std::array::from_fn(|i| f[i] - bg[i] as f32);
                let denom: f32 = fb.iter().map(|v| v * v).sum();
                let far = fb.iter().fold(0f32, |m, v| m.max(v.abs()));
                if far < (2 * tol) as f32 {
                    1.0 // 인형 색이 배경과 거의 같다: 섞인 비율을 알 수 없다.
                } else {
                    let cb: f32 = (0..3).map(|i| (c[i] as f32 - bg[i] as f32) * fb[i]).sum();
                    (cb / denom).clamp(0.0, 1.0)
                }
            } else {
                // 띠보다 얇은 부분(수염, 실 등): 배경과의 차이만으로 어림한다.
                let span = (0..3).map(|i| (bg[i] as i32).max(255 - bg[i] as i32)).max().unwrap_or(255).max(1) as f32;
                ((color_dist(c, bg) - tol) as f32 / (span * 0.5)).clamp(0.0, 1.0)
            };
            if a >= 0.98 {
                continue;
            }
            let mut px = [0u8; 4];
            if a > 0.03 {
                for i in 0..3 {
                    px[i] = ((c[i] as f32 - (1.0 - a) * bg[i] as f32) / a).round().clamp(0.0, 255.0) as u8;
                }
                px[3] = (a * 255.0).round() as u8;
            }
            out.push((x, y, px));
        }
    }
    out
}

/// 배경에 남은 작은 얼룩(가장 큰 덩어리의 1/150보다 작고 떨어져 있는 불투명 조각)을 지운다.
/// 얼룩 하나 때문에 여백이 덜 잘리는 일을 막는다.
fn drop_specks(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    let n = (w * h) as usize;
    let solid: Vec<bool> = img.pixels().map(|p| p.0[3] > ALPHA_MIN).collect();
    let mut label = vec![u32::MAX; n];
    let mut sizes: Vec<usize> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..n {
        if !solid[start] || label[start] != u32::MAX {
            continue;
        }
        let id = sizes.len() as u32;
        let mut size = 0;
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = ((i as u32 % w) as i64, (i as u32 / w) as i64);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = (ny as u32 * w + nx as u32) as usize;
                    if solid[j] && label[j] == u32::MAX {
                        label[j] = id;
                        stack.push(j);
                    }
                }
            }
        }
        sizes.push(size);
    }
    let Some(&largest) = sizes.iter().max() else { return };
    let min = (largest / 150).max(9);
    for (i, l) in label.iter().enumerate() {
        if *l != u32::MAX && sizes[*l as usize] < min {
            img.get_pixel_mut(i as u32 % w, i as u32 / w).0[3] = 0;
        }
    }
}

/// 이미지를 읽어(PNG/JPG/WebP/BMP/GIF 첫 프레임 등) 여백을 자르고 저장한다. (파일명, 너비/높이)를 돌려준다.
pub fn import(src: &Path, images_dir: &Path, id: &str) -> Result<(String, f32), String> {
    let reader = ImageReader::open(src).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut rgba = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    remove_flat_background(&mut rgba);
    let (x, y, w, h) = opaque_bounds(&rgba).ok_or("The image is fully transparent.")?;
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
    fn doll_touching_corners_and_bottom_still_gets_cut_out() {
        // 인형이 아래쪽 두 모서리까지 덮는다: 예전 방식(네 모서리가 같아야 함)으로는 못 했다.
        let mut img = RgbaImage::from_pixel(60, 80, Rgba([255, 255, 255, 255]));
        for y in 50..80 {
            for x in 0..60 {
                img.put_pixel(x, y, Rgba([150, 90, 60, 255]));
            }
        }
        remove_flat_background(&mut img);
        assert_eq!(img.get_pixel(30, 5).0[3], 0);
        assert_eq!(img.get_pixel(30, 65).0[3], 255);
        assert_eq!(opaque_bounds(&img), Some((0, 50, 60, 30)));
    }

    #[test]
    fn soft_edges_lose_the_white_fringe() {
        // 흰 바탕 위 빨간 원, 경계는 3픽셀에 걸쳐 흰색과 섞여 있다 (JPG·안티에일리어싱).
        let mut img = RgbaImage::new(80, 80);
        let red = [200.0f32, 30.0, 40.0];
        for (x, y, p) in img.enumerate_pixels_mut() {
            let r = ((x as f32 - 40.0).powi(2) + (y as f32 - 40.0).powi(2)).sqrt();
            let a = ((26.0 - r) / 3.0).clamp(0.0, 1.0);
            let mix = |i: usize| (a * red[i] + (1.0 - a) * 255.0).round() as u8;
            *p = Rgba([mix(0), mix(1), mix(2), 255]);
        }
        remove_flat_background(&mut img);
        let mut edge = 0;
        for p in img.pixels().filter(|p| p.0[3] > 0) {
            // 남은 픽셀은 반투명이어도 빨간색이어야 한다 (흰색이 섞인 분홍 테가 없어야 한다).
            assert!(p.0[1] < 70 && p.0[2] < 80, "fringe pixel {:?}", p.0);
            edge += (p.0[3] < 250) as u32;
        }
        assert!(edge > 50, "edge should stay soft ({edge} semi-transparent pixels)");
        assert_eq!(img.get_pixel(40, 40).0, [200, 30, 40, 255]);
    }

    #[test]
    fn fake_transparency_checkerboard_is_removed() {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let v = if (x / 8 + y / 8) % 2 == 0 { 255 } else { 204 };
            *p = Rgba([v, v, v, 255]);
            if (20..44).contains(&x) && (16..56).contains(&y) {
                *p = Rgba([220, 60, 90, 255]);
            }
        }
        remove_flat_background(&mut img);
        assert_eq!(img.get_pixel(2, 2).0[3], 0);
        assert_eq!(img.get_pixel(10, 2).0[3], 0);
        assert_eq!(img.get_pixel(60, 60).0[3], 0);
        assert_eq!(img.get_pixel(30, 30).0[3], 255);
        assert_eq!(opaque_bounds(&img), Some((20, 16, 24, 40)));
    }

    #[test]
    fn enclosed_background_and_specks_are_removed_but_small_white_parts_stay() {
        // 고리 모양 인형: 가운데 넓은 구멍은 배경, 몸 위 작은 흰 점(눈)은 남는다. 구석의 얼룩은 지운다.
        let mut img = RgbaImage::from_pixel(80, 80, Rgba([255, 255, 255, 255]));
        for y in 10..70 {
            for x in 10..70 {
                let hole = (25..55).contains(&x) && (25..55).contains(&y);
                if !hole {
                    img.put_pixel(x, y, Rgba([40, 90, 160, 255]));
                }
            }
        }
        for (x, y) in [(15, 15), (16, 15), (15, 16), (16, 16)] {
            img.put_pixel(x, y, Rgba([255, 255, 255, 255])); // 눈 (4픽셀)
        }
        for (x, y) in [(2, 77), (3, 77), (2, 76)] {
            img.put_pixel(x, y, Rgba([10, 10, 10, 255])); // 얼룩
        }
        remove_flat_background(&mut img);
        assert_eq!(img.get_pixel(40, 40).0[3], 0, "hole must be cleared");
        assert_eq!(img.get_pixel(15, 15).0[3], 255, "small white eye must stay");
        assert_eq!(img.get_pixel(2, 77).0[3], 0, "speck must be cleared");
        assert_eq!(opaque_bounds(&img), Some((10, 10, 60, 60)));
    }

    #[test]
    fn photos_and_transparent_images_are_left_alone() {
        // 테두리 색이 제각각이면(사진) 단색 배경이 아니다.
        let mut photo = RgbaImage::new(30, 30);
        for (x, y, p) in photo.enumerate_pixels_mut() {
            *p = Rgba([(x * 8) as u8, (y * 8) as u8, ((x + y) * 4) as u8, 255]);
        }
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
