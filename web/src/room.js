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
