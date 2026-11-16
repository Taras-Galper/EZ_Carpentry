//! Room floor plan (polygon of wall faces with doors and windows) and
//! automatic filling of a wall with cabinets.
//!
//! Plan coordinates: x to the right, z towards the viewer (down on the
//! plan). `walls` lists the inside corners of the room in order; wall i
//! runs from corner i to corner i+1 (closing back to corner 0). Walls are
//! stored so the room is on the left of each wall's direction in that
//! frame, i.e. a cabinet backed against wall i faces along its normal.

use crate::model::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    #[default]
    Door,
    Window,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Opening {
    pub wall: usize,
    pub kind: OpeningKind,
    /// Distance from the wall's start corner to the opening.
    pub offset: f64,
    pub width: f64,
    pub height: f64,
    /// Height of the window sill above the floor (0 for doors).
    pub sill: f64,
}

impl Default for Opening {
    fn default() -> Self {
        Self { wall: 0, kind: OpeningKind::Door, offset: 300.0, width: 800.0, height: 2050.0, sill: 0.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Room {
    /// Inside corners, in order. Fewer than 3 = no room.
    pub walls: Vec<[f64; 2]>,
    pub thickness: f64,
    pub height: f64,
    pub openings: Vec<Opening>,
}

impl Default for Room {
    fn default() -> Self {
        Self { walls: vec![], thickness: 100.0, height: 2600.0, openings: vec![] }
    }
}

/// One wall in plan: start, unit direction, length, inward normal.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Wall {
    pub start: [f64; 2],
    pub dir: [f64; 2],
    pub len: f64,
    pub normal: [f64; 2],
    /// Cabinet rotation (degrees) that puts its back on this wall.
    pub rotation: f64,
}

impl Room {
    pub fn rectangle(w: f64, d: f64) -> Self {
        Self { walls: vec![[0.0, 0.0], [w, 0.0], [w, d], [0.0, d]], ..Self::default() }
    }

    /// Twice the signed area; positive when walls run the expected way.
    fn signed_area2(&self) -> f64 {
        let n = self.walls.len();
        (0..n)
            .map(|i| {
                let (a, b) = (self.walls[i], self.walls[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum()
    }

    /// Reverse the corner order if needed so every wall's normal points
    /// into the room. Openings are re-indexed and re-measured.
    pub fn orient(&mut self) {
        if self.walls.len() < 3 || self.signed_area2() >= 0.0 {
            return;
        }
        let n = self.walls.len();
        let lens: Vec<f64> = (0..n).map(|i| self.wall(i).map(|w| w.len).unwrap_or(0.0)).collect();
        self.walls.reverse();
        // old wall i (corner i -> i+1) becomes new wall n-2-i (mod n), running backwards
        for o in self.openings.iter_mut() {
            let old = o.wall % n;
            o.wall = (2 * n - 2 - old) % n;
            o.offset = lens[old] - o.offset - o.width;
        }
    }

    pub fn wall(&self, i: usize) -> Option<Wall> {
        let n = self.walls.len();
        if n < 3 || i >= n {
            return None;
        }
        let (a, b) = (self.walls[i], self.walls[(i + 1) % n]);
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dz * dz).sqrt();
        if len < 1e-6 {
            return None;
        }
        let dir = [dx / len, dz / len];
        // cabinet local x = (cos r, -sin r) along the wall
        let rotation = (-dir[1]).atan2(dir[0]).to_degrees() + 0.0; // no -0
        Some(Wall { start: a, dir, len, normal: [-dir[1], dir[0]], rotation })
    }

    pub fn walls(&self) -> Vec<Wall> {
        (0..self.walls.len()).filter_map(|i| self.wall(i)).collect()
    }
}

impl Wall {
    /// Plan point -> (along the wall from its start, out from the wall into the room).
    pub fn local(&self, x: f64, z: f64) -> (f64, f64) {
        let (dx, dz) = (x - self.start[0], z - self.start[1]);
        (dx * self.dir[0] + dz * self.dir[1], dx * self.normal[0] + dz * self.normal[1])
    }
    pub fn point(&self, t: f64, s: f64) -> (f64, f64) {
        (
            self.start[0] + self.dir[0] * t + self.normal[0] * s,
            self.start[1] + self.dir[1] * t + self.normal[1] * s,
        )
    }
}

/// Which band of the wall a fill targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Base,
    Wall,
}

const BASE_DEPTH: f64 = 560.0;
const BASE_HEIGHT: f64 = 870.0;
const WALL_DEPTH: f64 = 320.0;
const WALL_HEIGHT: f64 = 720.0;
const WALL_ELEVATION: f64 = 1450.0;
const WIDTHS: [f64; 8] = [1000.0, 900.0, 800.0, 600.0, 500.0, 450.0, 400.0, 300.0];
/// Leftovers narrower than this are left as a gap.
const MIN_FILLER: f64 = 20.0;

fn footprint(c: &Cabinet) -> [(f64, f64); 4] {
    let p = |x: f64, z: f64| crate::parts::to_world(c.x, c.z, c.rotation, x, z);
    [p(0.0, 0.0), p(c.width, 0.0), p(0.0, c.depth), p(c.width, c.depth)]
}

/// Does `c` occupy the given height band?
fn in_band(c: &Cabinet, lo: f64, hi: f64) -> bool {
    let (b, t) = (c.elevation, c.elevation + c.height);
    b < hi && t > lo
}

/// Blocked intervals along the wall for a band of height [lo, hi] and depth `depth`.
fn blocked(project: &Project, wi: usize, w: &Wall, lo: f64, hi: f64, depth: f64) -> Vec<(f64, f64)> {
    let mut out = vec![];
    for o in project.room.openings.iter().filter(|o| o.wall == wi) {
        let (b, t) = (o.sill, o.sill + o.height);
        if b < hi && t > lo {
            out.push((o.offset, o.offset + o.width));
        }
    }
    for c in &project.cabinets {
        if !in_band(c, lo, hi) {
            continue;
        }
        let pts: Vec<(f64, f64)> = footprint(c).iter().map(|&(x, z)| w.local(x, z)).collect();
        let (t0, t1) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
        let (s0, s1) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
        if s1 > 1.0 && s0 < depth - 1.0 && t1 > 0.0 && t0 < w.len {
            out.push((t0, t1));
        }
    }
    // Other walls that cut into this wall's strip (acute corners, alcoves).
    for (j, ow) in project.room.walls().iter().enumerate() {
        if j == wi {
            continue;
        }
        for k in 0..=20 {
            let (x, z) = (ow.start[0] + ow.dir[0] * ow.len * k as f64 / 20.0, ow.start[1] + ow.dir[1] * ow.len * k as f64 / 20.0);
            let (t, s) = w.local(x, z);
            if s > 1.0 && s < depth - 1.0 && t > 0.0 && t < w.len {
                out.push((t - 1.0, t + 1.0));
            }
        }
    }
    out
}

/// Free stretches of [0, len] once `blocked` is removed.
fn free(len: f64, mut blocked: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    blocked.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = vec![];
    let mut cur = 0.0;
    for (a, b) in blocked {
        if a > cur {
            out.push((cur, a.min(len)));
        }
        cur = cur.max(b);
    }
    if cur < len {
        out.push((cur, len));
    }
    out.into_iter().filter(|(a, b)| b - a >= 1.0).collect()
}

/// Standard widths that best fill `len`: most length used, then fewest
/// cabinets, then most 600s. Returns widths, widest first.
pub fn pack(len: f64) -> Vec<f64> {
    const STEP: f64 = 5.0;
    let n = (len / STEP).floor() as usize;
    let units: Vec<usize> = WIDTHS.iter().map(|w| (w / STEP) as usize).collect();
    // best[s] = (count, -sixes, previous width index)
    let mut best: Vec<Option<(u32, i32, usize)>> = vec![None; n + 1];
    best[0] = Some((0, 0, usize::MAX));
    for s in 1..=n {
        for (k, &u) in units.iter().enumerate() {
            if u > s {
                continue;
            }
            if let Some((c, six, _)) = best[s - u] {
                let cand = (c + 1, six - (WIDTHS[k] == 600.0) as i32, k);
                if best[s].is_none_or(|b| (cand.0, cand.1) < (b.0, b.1)) {
                    best[s] = Some(cand);
                }
            }
        }
    }
    let Some(s) = (0..=n).rev().find(|&s| best[s].is_some()) else { return vec![] };
    let mut out = vec![];
    let mut s = s;
    while s > 0 {
        let (_, _, k) = best[s].expect("reachable");
        out.push(WIDTHS[k]);
        s -= units[k];
    }
    out.sort_by(|a, b| b.total_cmp(a));
    out
}

fn next_id(project: &Project, extra: &[Cabinet], prefix: &str) -> String {
    let mut i = 1;
    loop {
        let id = format!("{prefix}{i}");
        if !project.cabinets.iter().chain(extra).any(|c| c.id == id) {
            return id;
        }
        i += 1;
    }
}

/// Cabinets that fill wall `wi` in the given tier, leaving doors, windows,
/// corners and existing cabinets clear. Leftovers become fillers.
pub fn fill_wall(project: &Project, wi: usize, tier: Tier) -> Result<Vec<Cabinet>, String> {
    let w = project.room.wall(wi).ok_or("no such wall")?;
    let (lo, hi, depth, height, elev) = match tier {
        Tier::Base => (0.0, BASE_HEIGHT + 40.0, BASE_DEPTH, BASE_HEIGHT, 0.0),
        Tier::Wall => (WALL_ELEVATION, WALL_ELEVATION + WALL_HEIGHT, WALL_DEPTH, WALL_HEIGHT, WALL_ELEVATION),
    };
    let style = project.cabinets.first().map(|c| c.door_style).unwrap_or_default();
    let mut out: Vec<Cabinet> = vec![];
    let spans = free(w.len, blocked(project, wi, &w, lo, hi, depth));
    // A gap beside a door or window is normal; fillers only close gaps
    // against walls and cabinets.
    let at_opening = |t: f64| {
        project.room.openings.iter().any(|o| {
            o.wall == wi && o.sill < hi && o.sill + o.height > lo && ((o.offset - t).abs() < 1.0 || (o.offset + o.width - t).abs() < 1.0)
        })
    };
    for (a, b) in spans {
        let len = b - a;
        if len < 300.0 - 1e-6 {
            if len >= MIN_FILLER && !at_opening(a) && !at_opening(b) {
                out.push(filler(project, &out, &w, a, len, tier, height, elev, depth, style));
            }
            continue;
        }
        let widths = pack(len);
        let used: f64 = widths.iter().sum();
        let left = len - used;
        // Leftover goes into a filler at the end that meets a wall or
        // obstacle; the wall end gets it when both ends are bounded.
        let mut t = a;
        // Filler at the start only when the start meets the wall end and
        // the far end does not, or when the far end is an opening.
        let filler_first = left >= MIN_FILLER && !at_opening(a) && ((a <= 1.0 && b < w.len - 1.0) || at_opening(b));
        let filler_last = left >= MIN_FILLER && !filler_first && !at_opening(b);
        if filler_first {
            out.push(filler(project, &out, &w, t, left, tier, height, elev, depth, style));
            t += left;
        }
        for &cw in &widths {
            let (x, z) = w.point(t, 0.0);
            let id = next_id(project, &out, if tier == Tier::Base { "b" } else { "w" });
            let mut c = Cabinet {
                id,
                name: match tier {
                    Tier::Base => "Base".into(),
                    Tier::Wall => "Wall".into(),
                },
                kind: if tier == Tier::Base { CabinetKind::Base } else { CabinetKind::Wall },
                width: cw,
                height,
                depth,
                x: (x * 10.0).round() / 10.0,
                z: (z * 10.0).round() / 10.0,
                rotation: (w.rotation * 1000.0).round() / 1000.0,
                elevation: elev,
                door_style: style,
                doors: if cw >= 700.0 { 2 } else { 1 },
                shelves: if tier == Tier::Wall { 2 } else { 1 },
                drawers: if tier == Tier::Base && cw >= 400.0 && cw <= 600.0 { vec![160.0] } else { vec![] },
                ..Cabinet::default()
            };
            if tier == Tier::Wall {
                c.toe_kick_height = 0.0;
            }
            out.push(c);
            t += cw;
        }
        if filler_last {
            out.push(filler(project, &out, &w, t, left, tier, height, elev, depth, style));
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn filler(
    project: &Project,
    made: &[Cabinet],
    w: &Wall,
    t: f64,
    width: f64,
    tier: Tier,
    height: f64,
    elev: f64,
    depth: f64,
    style: DoorStyle,
) -> Cabinet {
    let (x, z) = w.point(t, 0.0);
    Cabinet {
        id: next_id(project, made, "f"),
        name: "Filler".into(),
        kind: CabinetKind::Filler,
        width: (width * 10.0).round() / 10.0,
        height,
        depth,
        x: (x * 10.0).round() / 10.0,
        z: (z * 10.0).round() / 10.0,
        rotation: (w.rotation * 1000.0).round() / 1000.0,
        elevation: elev,
        toe_kick_height: if tier == Tier::Base { 100.0 } else { 0.0 },
        door_style: style,
        doors: 0,
        shelves: 0,
        ..Cabinet::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_room(w: f64, d: f64) -> Project {
        Project { cabinets: vec![], room: Room::rectangle(w, d), ..sample_kitchen() }
    }

    #[test]
    fn pack_uses_standard_widths() {
        assert_eq!(pack(1200.0), vec![600.0, 600.0]);
        assert_eq!(pack(3000.0).iter().sum::<f64>(), 3000.0);
        let p = pack(2430.0);
        assert!(2430.0 - p.iter().sum::<f64>() < 50.0, "{p:?}");
        assert!(pack(250.0).is_empty());
    }

    #[test]
    fn rectangle_walls_face_inward() {
        let r = Room::rectangle(3000.0, 2000.0);
        let w: Vec<Wall> = r.walls();
        assert_eq!(w.len(), 4);
        assert_eq!(w[0].normal, [0.0, 1.0]); // back wall, room towards +z
        assert!((w[1].rotation + 90.0).abs() < 1e-9); // right wall like the sample's return leg
        // drawn the other way round it gets reoriented
        let mut r2 = Room { walls: r.walls.iter().rev().cloned().collect(), ..Room::default() };
        r2.openings.push(Opening { wall: 0, offset: 100.0, width: 800.0, ..Opening::default() });
        r2.orient();
        assert!(r2.signed_area2() > 0.0);
        // the door stays on the same physical wall at the same place
        let o = &r2.openings[0];
        let ww = r2.wall(o.wall).unwrap();
        let (x, z) = ww.point(o.offset, 0.0);
        assert!((z - 2000.0).abs() < 1e-6 || (x - 0.0).abs() < 1e-6 || (z).abs() < 1e-6 || (x - 3000.0).abs() < 1e-6);
    }

    #[test]
    fn fill_back_wall_exactly() {
        let p = empty_room(3000.0, 2500.0);
        let cabs = fill_wall(&p, 0, Tier::Base).unwrap();
        let total: f64 = cabs.iter().map(|c| c.width).sum();
        assert_eq!(total, 3000.0);
        assert!(cabs.iter().all(|c| c.z == 0.0 && c.rotation == 0.0));
        // contiguous from 0
        let mut x = 0.0;
        for c in &cabs {
            assert_eq!(c.x, x);
            x += c.width;
        }
    }

    #[test]
    fn fill_skips_door_and_uses_filler() {
        let mut p = empty_room(3000.0, 2500.0);
        p.room.openings.push(Opening { wall: 0, offset: 1000.0, width: 900.0, ..Opening::default() });
        let cabs = fill_wall(&p, 0, Tier::Base).unwrap();
        for c in &cabs {
            assert!(c.x + c.width <= 1000.0 + 1e-6 || c.x >= 1900.0 - 1e-6, "{} at {}..{}", c.id, c.x, c.x + c.width);
        }
        let total: f64 = cabs.iter().map(|c| c.width).sum();
        assert!((total - 2100.0).abs() < 1e-6, "{total}");
    }

    #[test]
    fn no_filler_beside_openings() {
        // sample: wall cabinets either side of the window over the sink
        let p = sample_kitchen();
        let cabs = fill_wall(&p, 0, Tier::Wall).unwrap();
        assert!(cabs.iter().all(|c| c.kind != CabinetKind::Filler), "{:?}", cabs.iter().map(|c| (&c.id, c.x, c.width)).collect::<Vec<_>>());
        // the free end of the wall (2600..3200) gets a 600 wall cabinet
        assert_eq!(cabs.iter().map(|c| (c.x, c.width)).collect::<Vec<_>>(), vec![(2600.0, 600.0)]);
    }

    #[test]
    fn window_blocks_wall_cabinets_only() {
        let mut p = empty_room(3000.0, 2500.0);
        p.room.openings.push(Opening { wall: 0, kind: OpeningKind::Window, offset: 1000.0, width: 1000.0, height: 1200.0, sill: 1000.0 });
        let base: f64 = fill_wall(&p, 0, Tier::Base).unwrap().iter().map(|c| c.width).sum();
        let wall: f64 = fill_wall(&p, 0, Tier::Wall).unwrap().iter().map(|c| c.width).sum();
        assert_eq!(base, 3000.0);
        assert_eq!(wall, 2000.0);
    }

    #[test]
    fn second_wall_starts_after_first_run() {
        let mut p = empty_room(3000.0, 2500.0);
        let back = fill_wall(&p, 0, Tier::Base).unwrap();
        p.cabinets.extend(back);
        let right = fill_wall(&p, 1, Tier::Base).unwrap();
        // right wall runs from z=0 down; must start past the back run's depth
        let first = right.iter().map(|c| c.z).fold(f64::MAX, f64::min);
        assert!(first >= 560.0 - 1e-6, "{first}");
        p.cabinets.extend(right);
        // and the whole project builds without warnings, with an L worktop pair
        let r = crate::compute(&p);
        assert!(r.build.warnings.is_empty(), "{:?}", r.build.warnings);
        assert_eq!(r.build.parts.iter().filter(|q| q.role == crate::parts::Role::Worktop).count(), 2);
    }

    #[test]
    fn angled_wall_cabinets_follow_it() {
        let mut p = empty_room(1.0, 1.0);
        p.room = Room { walls: vec![[0.0, 0.0], [2000.0, 0.0], [3414.2, 1414.2], [0.0, 3000.0]], ..Room::default() };
        p.room.orient();
        let cabs = fill_wall(&p, 1, Tier::Base).unwrap();
        assert!(!cabs.is_empty());
        for c in &cabs {
            assert!((c.rotation + 45.0).abs() < 0.01, "{}", c.rotation);
        }
    }
}
