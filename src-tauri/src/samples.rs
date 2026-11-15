//! 첫 실행 때 보여 줄 도형 인형 3개 (직접 그린 SVG).

use crate::config::{BandId, React, Doll, Manual};
use std::path::Path;

const SAMPLES: [(&str, f32, &str); 3] = [
    (
        "sample-1.svg",
        1.0,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="400" viewBox="0 0 200 200">
<circle cx="42" cy="38" r="34" fill="#f48fb1"/><circle cx="158" cy="38" r="34" fill="#f48fb1"/>
<circle cx="42" cy="40" r="17" fill="#fbd0df"/><circle cx="158" cy="40" r="17" fill="#fbd0df"/>
<path d="M0 124C0 62 45 22 100 22s100 40 100 102c0 50-38 76-100 76S0 174 0 124Z" fill="#f8a5c2"/>
<ellipse cx="100" cy="150" rx="46" ry="34" fill="#fde3ec"/>
<circle cx="66" cy="106" r="9" fill="#3b2a33"/><circle cx="134" cy="106" r="9" fill="#3b2a33"/>
<circle cx="69" cy="103" r="3" fill="#fff"/><circle cx="137" cy="103" r="3" fill="#fff"/>
<ellipse cx="100" cy="128" rx="8" ry="5.5" fill="#3b2a33"/>
<circle cx="44" cy="132" r="11" fill="#f06292" opacity=".45"/><circle cx="156" cy="132" r="11" fill="#f06292" opacity=".45"/>
</svg>"##,
    ),
    (
        "sample-2.svg",
        0.74,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="326" height="440" viewBox="0 0 163 220">
<path d="M0 140C0 62 30 0 81.500 0S163 62 163 140c0 54-34 80-81.500 80S0 194 0 140Z" fill="#7fd8be"/>
<ellipse cx="81.500" cy="162" rx="44" ry="42" fill="#d9f5ec"/>
<circle cx="56" cy="92" r="8" fill="#23423a"/><circle cx="107" cy="92" r="8" fill="#23423a"/>
<circle cx="58.500" cy="89.500" r="2.800" fill="#fff"/><circle cx="109.500" cy="89.500" r="2.800" fill="#fff"/>
<path d="M70 112q11.500 10 23 0" fill="none" stroke="#23423a" stroke-width="4" stroke-linecap="round"/>
<path d="M81.500 0c-6-14 8-20 14-12" fill="none" stroke="#4fb89a" stroke-width="6" stroke-linecap="round" transform="translate(0 16)"/>
</svg>"##,
    ),
    (
        "sample-3.svg",
        1.3,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="520" height="400" viewBox="0 0 260 200">
<path d="M0 132C0 70 44 30 130 30s130 40 130 102c0 46-50 68-130 68S0 178 0 132Z" fill="#ffd166"/>
<path d="M52 44 40 2l44 30ZM208 44l12-42-44 30Z" fill="#ffd166"/>
<path d="M54 36 46 16l22 16ZM206 36l8-20-22 16Z" fill="#ffe9ad"/>
<ellipse cx="130" cy="150" rx="62" ry="36" fill="#fff3d0"/>
<circle cx="88" cy="108" r="9" fill="#4a3b12"/><circle cx="172" cy="108" r="9" fill="#4a3b12"/>
<circle cx="91" cy="105" r="3" fill="#fff"/><circle cx="175" cy="105" r="3" fill="#fff"/>
<path d="M118 126q12 10 24 0" fill="none" stroke="#4a3b12" stroke-width="4" stroke-linecap="round"/>
<path d="M130 120v8" stroke="#4a3b12" stroke-width="4" stroke-linecap="round"/>
</svg>"##,
    ),
];

/// 샘플 SVG를 images 폴더에 쓰고 인형 목록을 돌려준다.
pub fn install(images_dir: &Path) -> Vec<Doll> {
    let _ = std::fs::create_dir_all(images_dir);
    SAMPLES
        .iter()
        .enumerate()
        .filter(|(_, (file, _, svg))| std::fs::write(images_dir.join(file), svg).is_ok())
        .map(|(i, (file, aspect, _))| Doll {
            id: format!("sample{}", i + 1),
            image: file.to_string(),
            aspect: *aspect,
            order: i as u32,
            flip: false,
            visible: true,
            band: BandId::Full,
            react: React::Beat,
            x: 0.5,
            y: 0.8,
            scale: 0.2,
            size: 1.0,
            line: String::new(),
            at: None,
            group: None,
            manual: Manual::default(),
        })
        .collect()
}
