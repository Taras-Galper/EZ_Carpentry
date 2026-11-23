//! casework CLI
//!
//!   casework sample                 print the sample kitchen project JSON
//!   casework export <project.json> [-o DIR] [--zip FILE]
//!   casework summary <project.json>
//!   casework walls <project.json>
//!   casework fill <project.json> <wall> [base|wall|both] [-o OUT.json]
//!   casework layout <project.json> <straight|l|u> <wall> [-o OUT.json]
//!   casework import-room <project.json> <plan.dxf>

use casework_core as cw;
use std::{fs, path::PathBuf, process::exit};

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    exit(1)
}

/// Today's date (UTC) as YYYY-MM-DD, for the shop packet.
fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let z = (secs / 86400) as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02}")
}

fn load(path: &str) -> cw::Project {
    let s = fs::read_to_string(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
    cw::parse(&s).unwrap_or_else(|e| die(format!("{path}: {e}")))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("sample") => {
            println!("{}", serde_json::to_string_pretty(&cw::sample_kitchen()).unwrap());
        }
        Some("summary") => {
            let p = load(args.get(1).unwrap_or_else(|| die("missing project.json")));
            let r = cw::compute(&p);
            println!("{}", serde_json::to_string_pretty(&r.summary).unwrap());
            for w in &r.build.warnings {
                eprintln!("warning: {w}");
            }
        }
        Some("walls") => {
            let p = load(args.get(1).unwrap_or_else(|| die("missing project.json")));
            for (i, w) in p.room.walls().iter().enumerate() {
                let doors = p.room.openings.iter().filter(|o| o.wall == i).count();
                println!(
                    "wall {i}: {:.0} mm from ({:.0}, {:.0}), cabinet rotation {:.1}°, {doors} opening(s)",
                    w.len, w.start[0], w.start[1], w.rotation
                );
            }
        }
        Some("fill") => {
            use cw::room::{fill_wall, Tier};
            let path = args.get(1).unwrap_or_else(|| die("missing project.json"));
            let wall: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or_else(|| die("missing wall number (see `casework walls`)"));
            let mut out = PathBuf::from(path);
            let mut tier = "both";
            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "-o" => {
                        out = PathBuf::from(args.get(i + 1).unwrap_or_else(|| die("-o needs a file")));
                        i += 1;
                    }
                    t @ ("base" | "wall" | "both") => tier = t,
                    a => die(format!("unknown argument {a}")),
                }
                i += 1;
            }
            let mut p = load(path);
            let tiers: &[Tier] = match tier {
                "base" => &[Tier::Base],
                "wall" => &[Tier::Wall],
                _ => &[Tier::Base, Tier::Wall],
            };
            let before = p.cabinets.len();
            for &t in tiers {
                let new = fill_wall(&p, wall, t).unwrap_or_else(|e| die(e));
                p.cabinets.extend(new);
            }
            fs::write(&out, serde_json::to_string_pretty(&p).unwrap()).unwrap_or_else(|e| die(e));
            println!("{} cabinets added -> {}", p.cabinets.len() - before, out.display());
        }
        Some("layout") => {
            use cw::room::{layout, Layout};
            let path = args.get(1).unwrap_or_else(|| die("missing project.json"));
            let kind = match args.get(2).map(|s| s.to_ascii_lowercase()).as_deref() {
                Some("straight") => Layout::Straight,
                Some("l") => Layout::L,
                Some("u") => Layout::U,
                _ => die("layout must be straight, l or u"),
            };
            let wall: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or_else(|| die("missing wall number (see `casework walls`)"));
            let out = match (args.get(4).map(String::as_str), args.get(5)) {
                (Some("-o"), Some(f)) => PathBuf::from(f),
                (None, _) => PathBuf::from(path),
                _ => die("expected -o OUT.json"),
            };
            let mut p = load(path);
            let before = p.cabinets.len();
            p.cabinets = layout(&p, kind, wall).unwrap_or_else(|e| die(e));
            fs::write(&out, serde_json::to_string_pretty(&p).unwrap()).unwrap_or_else(|e| die(e));
            println!("{} cabinets added -> {}", p.cabinets.len() - before, out.display());
        }
        Some("import-room") => {
            let path = args.get(1).unwrap_or_else(|| die("missing project.json"));
            let plan = args.get(2).unwrap_or_else(|| die("missing plan.dxf"));
            let mut p = load(path);
            let text = fs::read_to_string(plan).unwrap_or_else(|e| die(format!("{plan}: {e}")));
            let (room, info) = cw::import_dxf::room_from_dxf(&text).unwrap_or_else(|e| die(e));
            p.room = cw::room::Room { height: p.room.height, thickness: p.room.thickness, ..room };
            fs::write(path, serde_json::to_string_pretty(&p).unwrap()).unwrap_or_else(|e| die(e));
            println!("room imported: {info}");
        }
        Some("export") => {
            let path = args.get(1).unwrap_or_else(|| die("missing project.json"));
            let mut out = PathBuf::from("casework-out");
            let mut zip: Option<PathBuf> = None;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "-o" => out = PathBuf::from(args.get(i + 1).unwrap_or_else(|| die("-o needs a dir"))),
                    "--zip" => zip = Some(PathBuf::from(args.get(i + 1).unwrap_or_else(|| die("--zip needs a file")))),
                    a => die(format!("unknown argument {a}")),
                }
                i += 2;
            }
            let p = load(path);
            let r = cw::compute(&p);
            for w in &r.build.warnings {
                eprintln!("warning: {w}");
            }
            if let Some(z) = zip {
                fs::write(&z, cw::export::bundle_dated(&p, &r, Some(&today()))).unwrap_or_else(|e| die(e));
                println!("{}", z.display());
            } else {
                let files = cw::export::files_dated(&p, &r, Some(&today()));
                for (name, data) in &files {
                    let f = out.join(name);
                    if let Some(d) = f.parent() {
                        fs::create_dir_all(d).unwrap_or_else(|e| die(e));
                    }
                    fs::write(&f, data).unwrap_or_else(|e| die(e));
                }
                println!("{} files -> {}", files.len(), out.display());
            }
        }
        _ => {
            eprintln!("usage:\n  casework sample > kitchen.json\n  casework export kitchen.json [-o DIR] [--zip FILE]\n  casework summary kitchen.json\n  casework walls kitchen.json\n  casework fill kitchen.json <wall> [base|wall|both] [-o OUT.json]\n  casework layout kitchen.json <straight|l|u> <wall> [-o OUT.json]\n  casework import-room kitchen.json plan.dxf");
            exit(2)
        }
    }
}
