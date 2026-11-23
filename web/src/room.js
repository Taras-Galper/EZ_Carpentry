// Room geometry, mirroring crates/core/src/room.rs.
// Plan coordinates: x right, z towards the viewer. room.walls lists inside
// corners; wall i runs from corner i to corner i+1 and the room lies along
// its normal (-dir.z, dir.x).

export function wallOf(room, i) {
  const n = room.walls.length;
  if (n < 3 || i < 0 || i >= n) return null;
  const a = room.walls[i], b = room.walls[(i + 1) % n];
  const dx = b[0] - a[0], dz = b[1] - a[1], len = Math.hypot(dx, dz);
  if (len < 1e-6) return null;
  const dir = [dx / len, dz / len];
  return { i, start: a, end: b, dir, len, normal: [-dir[1], dir[0]], rotation: (Math.atan2(-dir[1], dir[0]) * 180) / Math.PI + 0 };
}

export const walls = (room) => room.walls.map((_, i) => wallOf(room, i)).filter(Boolean);

// Plan point -> [along the wall, out from the wall into the room]
export function local(w, x, z) {
  const dx = x - w.start[0], dz = z - w.start[1];
  return [dx * w.dir[0] + dz * w.dir[1], dx * w.normal[0] + dz * w.normal[1]];
}

export const point = (w, t, s) => [w.start[0] + w.dir[0] * t + w.normal[0] * s, w.start[1] + w.dir[1] * t + w.normal[1] * s];

// Solid stretches of a wall and the openings in it, for drawing.
// Returns [{t0, t1, y0, y1, glass}] along the wall.
export function wallPieces(room, w) {
  const H = room.height || 2600;
  const ops = (room.openings || []).filter((o) => o.wall === w.i).map((o) => ({
    t0: Math.max(0, o.offset), t1: Math.min(w.len, o.offset + o.width), y0: o.sill || 0, y1: (o.sill || 0) + o.height, window: o.kind === "window",
  })).filter((o) => o.t1 > o.t0).sort((a, b) => a.t0 - b.t0);
  const out = [];
  let t = 0;
  for (const o of ops) {
    if (o.t0 > t) out.push({ t0: t, t1: o.t0, y0: 0, y1: H });
    if (o.y0 > 0) out.push({ t0: o.t0, t1: o.t1, y0: 0, y1: o.y0 });
    if (o.y1 < H) out.push({ t0: o.t0, t1: o.t1, y0: o.y1, y1: H });
    if (o.window) out.push({ t0: o.t0, t1: o.t1, y0: o.y0, y1: Math.min(o.y1, H), glass: true });
    t = Math.max(t, o.t1);
  }
  if (t < w.len) out.push({ t0: t, t1: w.len, y0: 0, y1: H });
  return out;
}

// Snap a new corner: 10 mm grid, wall directions within 7° of a multiple of 45°.
export function snapCorner(prev, x, z) {
  x = Math.round(x / 10) * 10; z = Math.round(z / 10) * 10;
  if (!prev) return [x, z];
  const dx = x - prev[0], dz = z - prev[1], len = Math.hypot(dx, dz);
  if (len < 1) return [x, z];
  const a = Math.atan2(dz, dx), step = Math.PI / 4, k = Math.round(a / step);
  if (Math.abs(a - k * step) < (7 * Math.PI) / 180) {
    const L = Math.round(len / 10) * 10;
    return [Math.round(prev[0] + Math.cos(k * step) * L), Math.round(prev[1] + Math.sin(k * step) * L)];
  }
  return [x, z];
}

// Set wall i to length L, keeping `anchor` ("start" | "end" | "centre") in
// place. The neighbouring walls keep their directions: the moved corner
// slides its neighbour wall along, and the corner after that is found where
// that wall meets the next one, so a rectangle stays a rectangle.
// Returns new corners (rounded to 0.1 mm); openings on wall i keep their
// distance from the fixed point.
export function resizeWall(room, i, L, anchor = "start") {
  const n = room.walls.length;
  const P = room.walls.map((p) => [...p]);
  const w = wallOf(room, i);
  if (!w || !(L > 0)) return { walls: P, openings: room.openings };
  const at = (k) => ((k % n) + n) % n;
  const dirOf = (a, b) => { const dx = b[0] - a[0], dz = b[1] - a[1], l = Math.hypot(dx, dz) || 1; return [dx / l, dz / l]; };
  const cross = (a, b) => a[0] * b[1] - a[1] * b[0];
  // Line p + s d meets line q + t e.
  const meet = (p, d, q, e) => {
    const c = cross(d, e);
    if (Math.abs(c) < 1e-9) return null;
    const s = cross([q[0] - p[0], q[1] - p[1]], e) / c;
    return [p[0] + d[0] * s, p[1] + d[1] * s];
  };
  // Move corner k by v; its other wall (towards `side`) keeps its direction.
  const slide = (k, v, side) => {
    const far = at(k + side), beyond = at(k + 2 * side);
    const along = dirOf(P[k], P[far]), next = dirOf(P[far], P[beyond]);
    P[k] = [P[k][0] + v[0], P[k][1] + v[1]];
    if (n === 3 || far === i || far === at(i + 1)) return; // triangle: the far corner is fixed
    P[far] = meet(P[k], along, P[far], next) || [P[far][0] + v[0], P[far][1] + v[1]];
  };
  const d = L - w.len;
  const [ds, de] = anchor === "end" ? [d, 0] : anchor === "centre" ? [d / 2, d / 2] : [0, d];
  if (de) slide(at(i + 1), [w.dir[0] * de, w.dir[1] * de], +1);
  if (ds) slide(i, [-w.dir[0] * ds, -w.dir[1] * ds], -1);
  const shift = (list) => (list || []).map((o) => (o.wall === i && ds ? { ...o, offset: Math.round((o.offset + ds) * 10) / 10 } : o));
  return { walls: P.map((p) => [Math.round(p[0] * 10) / 10, Math.round(p[1] * 10) / 10]), openings: shift(room.openings), obstacles: shift(room.obstacles) };
}

// After the room's corners change, carry cabinets along with their walls.
// For each wall that kept its direction: every cabinet backed onto it moves
// with the wall line, and the chain of cabinets that starts at a corner
// that moved (first one within 60 mm of it, then touching neighbours)
// slides with that corner. Cabinets on walls that turned stay put.
// `cabinets` are mutated; returns how many were moved.
export function moveWithWalls(oldWalls, newWalls, cabinets) {
  const n = oldWalls.length;
  if (n !== newWalls.length || n < 3) return 0;
  const moved = new Set();
  for (let i = 0; i < n; i++) {
    const a = wallOf({ walls: oldWalls }, i), b = wallOf({ walls: newWalls }, i);
    if (!a || !b || a.dir[0] * b.dir[0] + a.dir[1] * b.dir[1] < 0.9999) continue;
    const ms = [b.start[0] - a.start[0], b.start[1] - a.start[1]], me = [b.end[0] - a.end[0], b.end[1] - a.end[1]];
    if (Math.hypot(...ms) < 0.01 && Math.hypot(...me) < 0.01) continue;
    const perp = ms[0] * a.normal[0] + ms[1] * a.normal[1]; // the wall line's shift into the room
    const alongS = ms[0] * a.dir[0] + ms[1] * a.dir[1], alongE = me[0] * a.dir[0] + me[1] * a.dir[1];
    const on = cabinets.filter((c) => {
      if (moved.has(c) || Math.abs(((c.rotation - a.rotation + 540) % 360) - 180) > 0.5) return false;
      const [t, s] = local(a, c.x, c.z);
      return Math.abs(s) < 50 && t > -c.width && t < a.len;
    }).map((c) => ({ c, t: local(a, c.x, c.z)[0] }));
    // chains from each end, per height tier; a chain that runs corner to
    // corner belongs to both ends and stays where it is
    const shift = new Map(on.map((o) => [o.c, 0]));
    for (const low of [true, false]) {
      const tier = on.filter((o) => (o.c.elevation < 500) === low).sort((p, q) => p.t - q.t);
      const fromS = new Set(), fromE = new Set();
      let edge = 0;
      for (const o of tier) { if (o.t - edge > 60) break; fromS.add(o.c); edge = o.t + o.c.width; }
      edge = a.len;
      for (const o of [...tier].reverse()) { if (edge - (o.t + o.c.width) > 60) break; fromE.add(o.c); edge = o.t; }
      for (const o of tier) {
        if (fromS.has(o.c) && !fromE.has(o.c)) shift.set(o.c, alongS);
        if (fromE.has(o.c) && !fromS.has(o.c)) shift.set(o.c, alongE);
      }
    }
    for (const { c } of on) {
      const d = shift.get(c);
      c.x = Math.round((c.x + a.normal[0] * perp + a.dir[0] * d) * 10) / 10;
      c.z = Math.round((c.z + a.normal[1] * perp + a.dir[1] * d) * 10) / 10;
      moved.add(c);
    }
  }
  return moved.size;
}
