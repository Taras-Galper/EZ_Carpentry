//! Room outline from a DXF floor plan.
//!
//! Reads LINE, LWPOLYLINE and POLYLINE/VERTEX entities (arcs are
//! ignored), joins lines into closed loops, and picks the room: the
//! largest loop, or - when walls are drawn with both faces - the largest
//! loop lying inside it (the inner face). Units come from $INSUNITS, or
//! are guessed from the size (metres, centimetres or millimetres).
//! Plan coordinates: x stays, DXF y becomes -z, and the outline is moved
//! so its corner is at the origin.

use crate::room::Room;
use std::collections::HashMap;

type P = (f64, f64);

struct Dxf {
    units: Option<i64>,
    segments: Vec<(P, P)>,
    loops: Vec<Vec<P>>,
}

fn read(text: &str) -> Dxf {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    let mut pairs = vec![];
    let mut i = 0;
    while i + 1 < lines.len() {
        if let Ok(code) = lines[i].parse::<i32>() {
            pairs.push((code, lines[i + 1]));
        }
        i += 2;
    }
    let mut out = Dxf { units: None, segments: vec![], loops: vec![] };
    let mut k = 0;
    let num = |s: &str| s.parse::<f64>().unwrap_or(0.0);
    while k < pairs.len() {
        let (c, v) = pairs[k];
        if c == 9 && v == "$INSUNITS" && k + 1 < pairs.len() {
            out.units = pairs[k + 1].1.parse().ok();
        }
        if c != 0 {
            k += 1;
            continue;
        }
        match v {
            "LINE" => {
                let (mut a, mut b) = ((0.0, 0.0), (0.0, 0.0));
                k += 1;
                while k < pairs.len() && pairs[k].0 != 0 {
                    match pairs[k].0 {
                        10 => a.0 = num(pairs[k].1),
                        20 => a.1 = num(pairs[k].1),
                        11 => b.0 = num(pairs[k].1),
                        21 => b.1 = num(pairs[k].1),
                        _ => {}
                    }
                    k += 1;
                }
                out.segments.push((a, b));
            }
            "LWPOLYLINE" => {
                let mut pts: Vec<P> = vec![];
                let mut closed = false;
                k += 1;
                while k < pairs.len() && pairs[k].0 != 0 {
                    match pairs[k].0 {
                        70 => closed = (pairs[k].1.parse::<i64>().unwrap_or(0) & 1) == 1,
                        10 => pts.push((num(pairs[k].1), 0.0)),
                        20 => {
                            if let Some(p) = pts.last_mut() {
                                p.1 = num(pairs[k].1);
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                add_poly(&mut out, pts, closed);
            }
            "POLYLINE" => {
                let mut closed = false;
                let mut pts: Vec<P> = vec![];
                k += 1;
                while k < pairs.len() && pairs[k].0 != 0 {
                    if pairs[k].0 == 70 {
                        closed = (pairs[k].1.parse::<i64>().unwrap_or(0) & 1) == 1;
                    }
                    k += 1;
                }
                while k < pairs.len() && pairs[k] == (0, "VERTEX") {
                    let mut p = (0.0, 0.0);
                    k += 1;
                    while k < pairs.len() && pairs[k].0 != 0 {
                        match pairs[k].0 {
                            10 => p.0 = num(pairs[k].1),
                            20 => p.1 = num(pairs[k].1),
                            _ => {}
                        }
                        k += 1;
                    }
                    pts.push(p);
                }
                add_poly(&mut out, pts, closed);
            }
            _ => k += 1,
        }
    }
    out
}

fn add_poly(out: &mut Dxf, mut pts: Vec<P>, closed: bool) {
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
    let first_last_same = pts.len() > 2 && dist(pts[0], *pts.last().unwrap()) < 1e-9;
    if first_last_same {
        pts.pop();
    }
    if (closed || first_last_same) && pts.len() >= 3 {
        out.loops.push(pts);
    } else {
        for w in pts.windows(2) {
            out.segments.push((w[0], w[1]));
        }
    }
}

fn dist(a: P, b: P) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Join segments into closed loops (endpoints within `tol`).
fn chain(segments: &[(P, P)], tol: f64) -> Vec<Vec<P>> {
    let key = |p: P| ((p.0 / tol).round() as i64, (p.1 / tol).round() as i64);
    let mut at: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, (a, b)) in segments.iter().enumerate() {
        if dist(*a, *b) < tol {
            continue;
        }
        at.entry(key(*a)).or_default().push(i);
        at.entry(key(*b)).or_default().push(i);
    }
    let mut used = vec![false; segments.len()];
    let mut loops = vec![];
    for s in 0..segments.len() {
        if used[s] || dist(segments[s].0, segments[s].1) < tol {
            continue;
        }
        let start = segments[s].0;
        let mut pts = vec![start];
        let mut cur = segments[s].1;
        used[s] = true;
        let mut closed = false;
        for _ in 0..segments.len() {
            if key(cur) == key(start) {
                closed = true;
                break;
            }
            pts.push(cur);
            let next = at.get(&key(cur)).and_then(|v| v.iter().copied().find(|&j| !used[j]));
            let Some(j) = next else { break };
            used[j] = true;
            let (a, b) = segments[j];
            cur = if key(a) == key(cur) { b } else { a };
        }
        if closed && pts.len() >= 3 {
            loops.push(pts);
        }
    }
    loops
}

fn area(p: &[P]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i].0 * p[(i + 1) % n].1 - p[(i + 1) % n].0 * p[i].1).sum::<f64>() / 2.0
}

fn inside(poly: &[P], q: P) -> bool {
    let n = poly.len();
    let mut c = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.1 > q.1) != (b.1 > q.1) && q.0 < (b.0 - a.0) * (q.1 - a.1) / (b.1 - a.1) + a.0 {
            c = !c;
        }
    }
    c
}

/// Drop corners where the outline runs straight on.
fn simplify(p: &[P]) -> Vec<P> {
    let n = p.len();
    let mut out = vec![];
    for i in 0..n {
        let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
        let len = dist(a, b) * dist(b, c);
        if len > 0.0 && (cross / len).abs() > 0.009 {
            out.push(b);
        }
    }
    out
}

/// Room from DXF text. Errors when no closed outline is found.
pub fn room_from_dxf(text: &str) -> Result<(Room, String), String> {
    let d = read(text);
    let mut loops = d.loops;
    let raw_ext = loops.iter().flatten().chain(d.segments.iter().flat_map(|s| [&s.0, &s.1]));
    let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for p in raw_ext {
        lo = (lo.0.min(p.0), lo.1.min(p.1));
        hi = (hi.0.max(p.0), hi.1.max(p.1));
    }
    if lo.0 > hi.0 {
        return Err("no lines or polylines in the DXF".into());
    }
    let ext = (hi.0 - lo.0).max(hi.1 - lo.1);
    let (scale, unit) = match d.units {
        Some(4) => (1.0, "mm"),
        Some(5) => (10.0, "cm"),
        Some(6) => (1000.0, "m"),
        Some(1) => (25.4, "inch"),
        Some(2) => (304.8, "ft"),
        _ if ext < 100.0 => (1000.0, "m (guessed)"),
        _ if ext < 3000.0 => (10.0, "cm (guessed)"),
        _ => (1.0, "mm (guessed)"),
    };
    loops.extend(chain(&d.segments, 0.5 / scale));
    let mut loops: Vec<Vec<P>> = loops
        .into_iter()
        .map(|l| simplify(&l.iter().map(|p| (p.0 * scale, -p.1 * scale)).collect::<Vec<_>>()))
        .filter(|l| l.len() >= 3 && area(l).abs() > 250_000.0)
        .collect();
    if loops.is_empty() {
        return Err("no closed room outline (at least 0.5 × 0.5 m) found in the DXF".into());
    }
    loops.sort_by(|a, b| area(b).abs().total_cmp(&area(a).abs()));
    let outer = &loops[0];
    let a0 = area(outer).abs();
    // Walls drawn with both faces: take the inner face.
    let pick = loops[1..]
        .iter()
        .find(|l| area(l).abs() > 0.5 * a0 && l.iter().all(|&p| inside(outer, p)))
        .unwrap_or(outer)
        .clone();
    let minx = pick.iter().map(|p| p.0).fold(f64::MAX, f64::min);
    let minz = pick.iter().map(|p| p.1).fold(f64::MAX, f64::min);
    let walls: Vec<[f64; 2]> = pick.iter().map(|p| [((p.0 - minx) * 10.0).round() / 10.0, ((p.1 - minz) * 10.0).round() / 10.0]).collect();
    let mut room = Room { walls, ..Room::default() };
    room.orient();
    let n = room.walls.len();
    Ok((room, format!("{n} walls, units {unit}, {} loop(s) found", loops.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: P, b: P) -> String {
        format!("0\nLINE\n8\nWALLS\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a.0, a.1, b.0, b.1)
    }

    fn dxf(units: Option<i64>, body: &str) -> String {
        let h = units.map(|u| format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{u}\n0\nENDSEC\n")).unwrap_or_default();
        format!("{h}0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")
    }

    fn rect_lines(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
        // drawn in a shuffled order and direction, as CAD files often are
        [line((x1, y1), (x1, y0)), line((x0, y0), (x1, y0)), line((x0, y1), (x0, y0)), line((x0, y1), (x1, y1))].concat()
    }

    #[test]
    fn double_line_walls_pick_inner_face() {
        let body = rect_lines(0.0, 0.0, 3400.0, 3000.0) + &rect_lines(100.0, 100.0, 3300.0, 2900.0);
        let (room, info) = room_from_dxf(&dxf(Some(4), &body)).unwrap();
        assert_eq!(room.walls.len(), 4, "{info}");
        let xs: Vec<f64> = room.walls.iter().map(|p| p[0]).collect();
        let zs: Vec<f64> = room.walls.iter().map(|p| p[1]).collect();
        assert_eq!(xs.iter().cloned().fold(f64::MIN, f64::max), 3200.0);
        assert_eq!(zs.iter().cloned().fold(f64::MIN, f64::max), 2800.0);
        assert!(room.walls().iter().all(|w| w.len > 0.0));
    }

    #[test]
    fn l_shaped_polyline_in_metres_without_units() {
        // L-shape 4 x 3 m with a 1.5 x 1 m notch, as a closed LWPOLYLINE, with a collinear extra vertex
        let pts = [(0.0, 0.0), (2.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.5, 2.0), (2.5, 3.0), (0.0, 3.0)];
        let mut body = String::from("0\nLWPOLYLINE\n8\nROOM\n90\n7\n70\n1\n");
        for (x, y) in pts {
            body += &format!("10\n{x}\n20\n{y}\n");
        }
        let (room, info) = room_from_dxf(&dxf(None, &body)).unwrap();
        assert!(info.contains("m (guessed)"), "{info}");
        assert_eq!(room.walls.len(), 6, "collinear point dropped");
        let lens: Vec<f64> = room.walls().iter().map(|w| (w.len * 10.0).round() / 10.0).collect();
        assert_eq!(lens.iter().sum::<f64>(), 14000.0);
        // walls face into the room
        let p = crate::Project { room, cabinets: vec![], ..crate::sample_kitchen() };
        let cabs = crate::room::fill_wall(&p, 0, crate::room::Tier::Base).unwrap();
        assert!(!cabs.is_empty());
    }

    #[test]
    fn no_outline_is_an_error() {
        assert!(room_from_dxf(&dxf(Some(4), &line((0.0, 0.0), (1000.0, 0.0)))).is_err());
    }
}
