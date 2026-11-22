//! Code 128 (set B) barcodes as SVG.

const PATTERNS: [&str; 107] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213",
    "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132",
    "221231", "213212", "223112", "312131", "311222", "321122", "321221", "312212", "322112", "322211",
    "212123", "212321", "232121", "111323", "131123", "131321", "112313", "132113", "132311", "211313",
    "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121", "313121", "211331",
    "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111",
    "314111", "221411", "431111", "111224", "111422", "121124", "121421", "141122", "141221", "112214",
    "112412", "122114", "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111",
    "111242", "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
    "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311", "113141",
    "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
];
const START_B: usize = 104;
const STOP: usize = 106;

/// Module widths (bar, space, bar, ...) for `text`, Code 128 set B.
/// Characters outside ASCII 32..=126 are replaced by '?'.
pub fn modules(text: &str) -> Vec<u8> {
    let vals: Vec<usize> = text
        .chars()
        .map(|c| {
            let c = if (' '..='~').contains(&c) { c } else { '?' };
            c as usize - 32
        })
        .collect();
    let check = (START_B + vals.iter().enumerate().map(|(i, v)| (i + 1) * v).sum::<usize>()) % 103;
    let mut out = vec![];
    for code in std::iter::once(START_B).chain(vals).chain([check, STOP]) {
        out.extend(PATTERNS[code].bytes().map(|b| b - b'0'));
    }
    out
}

/// Barcode as an SVG fragment `width` x `height` (user units) at (x, y),
/// with a quiet zone of 10 modules each side included in `width`.
pub fn svg(text: &str, x: f64, y: f64, width: f64, height: f64) -> String {
    let m = modules(text);
    let total: u32 = m.iter().map(|&w| w as u32).sum::<u32>() + 20;
    let unit = width / total as f64;
    let mut cx = x + 10.0 * unit;
    let mut o = String::new();
    for (i, &w) in m.iter().enumerate() {
        let wd = w as f64 * unit;
        if i % 2 == 0 {
            o.push_str(&format!(r#"<rect x="{cx:.3}" y="{y:.3}" width="{wd:.3}" height="{height:.3}"/>"#));
        }
        cx += wd;
    }
    format!(r##"<g fill="#000">{o}</g>"##)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure() {
        let m = modules("b2-door1");
        // start + 8 chars + check = 10 symbols of 11 modules, stop = 13
        assert_eq!(m.iter().map(|&w| w as u32).sum::<u32>(), 10 * 11 + 13);
        assert_eq!(m.len(), 10 * 6 + 7);
        for p in PATTERNS.iter().take(106) {
            assert_eq!(p.bytes().map(|b| (b - b'0') as u32).sum::<u32>(), 11);
        }
    }

    /// Prints module widths for comparison with an independent encoder.
    #[test]
    #[ignore]
    fn dump() {
        for t in ["b2-door1", "t1-side-l", "w1-d1sidel", "Hello 123", "x"] {
            let m: String = modules(t).iter().map(|w| w.to_string()).collect();
            println!("{t}={m}");
        }
    }
}
