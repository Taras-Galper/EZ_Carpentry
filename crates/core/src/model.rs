//! Project data model. Everything is in millimetres.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub name: String,
    pub settings: Settings,
    pub materials: Vec<Material>,
    pub cabinets: Vec<Cabinet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Saw / router cutter kerf used by nesting.
    pub kerf: f64,
    /// Trim removed from every sheet edge before nesting.
    pub sheet_trim: f64,
    /// Gap between adjacent fronts and around each front.
    pub front_gap: f64,
    /// Edge band thickness (subtracted from banded panel sizes).
    pub edge_band: f64,
    /// System-32 line: hole row distance from front / back edge.
    pub system_line: f64,
    /// Hinge cup centre distance from door edge.
    pub hinge_cup_inset: f64,
    /// Clearance per side for drawer slides.
    pub slide_clearance: f64,
    /// Handle hole spacing (centre to centre).
    pub handle_spacing: f64,
    /// Shelf front setback from carcass front.
    pub shelf_setback: f64,
    /// Worktop thickness over base cabinet runs; 0 = no worktop.
    pub worktop_thickness: f64,
    /// Worktop overhang past the fronts.
    pub worktop_overhang: f64,
    /// Shaker stile/rail width.
    pub shaker_frame: f64,
    /// Shaker panel groove depth (also the rail tenon length).
    pub shaker_groove: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            kerf: 6.0,
            sheet_trim: 10.0,
            front_gap: 3.0,
            edge_band: 0.0,
            system_line: 37.0,
            hinge_cup_inset: 22.5,
            slide_clearance: 13.0,
            handle_spacing: 128.0,
            shelf_setback: 20.0,
            worktop_thickness: 38.0,
            worktop_overhang: 20.0,
            shaker_frame: 60.0,
            shaker_groove: 8.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Material {
    pub id: String,
    pub name: String,
    pub thickness: f64,
    pub sheet_length: f64,
    pub sheet_width: f64,
    /// Grain runs along sheet length; parts may not be rotated.
    pub grain: bool,
    /// CSS hex colour for the 3D view.
    pub color: String,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            id: "carcass".into(),
            name: "White melamine 18".into(),
            thickness: 18.0,
            sheet_length: 2800.0,
            sheet_width: 2070.0,
            grain: false,
            color: "#eeeeea".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CabinetKind {
    Base,
    Wall,
    Tall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoorStyle {
    #[default]
    Slab,
    /// Five-piece frame: two stiles, two rails, a recessed panel.
    Shaker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HingeSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Cabinet {
    pub id: String,
    pub name: String,
    pub kind: CabinetKind,
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    /// Position of the cabinet's back-left-bottom corner in the room.
    pub x: f64,
    pub z: f64,
    /// Bottom elevation (wall cabinets hang above the floor).
    pub elevation: f64,
    /// Rotation about the vertical axis in degrees.
    pub rotation: f64,
    pub toe_kick_height: f64,
    pub toe_kick_depth: f64,
    /// Number of doors in the door zone: 0, 1 or 2.
    pub doors: u32,
    pub hinge_side: HingeSide,
    pub door_style: DoorStyle,
    /// Drawer front heights, top to bottom. Doors fill what remains.
    pub drawers: Vec<f64>,
    pub shelves: u32,
    pub carcass_material: String,
    pub back_material: String,
    pub front_material: String,
    pub drawer_material: String,
    /// Shaker centre panel.
    pub panel_material: String,
    pub handles: bool,
}

impl Default for Cabinet {
    fn default() -> Self {
        Self {
            id: "c1".into(),
            name: "Base".into(),
            kind: CabinetKind::Base,
            width: 600.0,
            height: 870.0,
            depth: 560.0,
            x: 0.0,
            z: 0.0,
            elevation: 0.0,
            rotation: 0.0,
            toe_kick_height: 100.0,
            toe_kick_depth: 60.0,
            doors: 1,
            hinge_side: HingeSide::Left,
            door_style: DoorStyle::Slab,
            drawers: vec![],
            shelves: 1,
            carcass_material: "carcass".into(),
            back_material: "back".into(),
            front_material: "front".into(),
            drawer_material: "drawer".into(),
            panel_material: "panel".into(),
            handles: true,
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        sample_kitchen()
    }
}

pub fn default_materials() -> Vec<Material> {
    vec![
        Material::default(),
        Material {
            id: "back".into(),
            name: "HDF white 6".into(),
            thickness: 6.0,
            sheet_length: 2800.0,
            sheet_width: 2070.0,
            grain: false,
            color: "#e4e2dc".into(),
        },
        Material {
            id: "front".into(),
            name: "Oak veneer MDF 19".into(),
            thickness: 19.0,
            sheet_length: 2800.0,
            sheet_width: 2070.0,
            grain: true,
            color: "#c49a6c".into(),
        },
        Material {
            id: "drawer".into(),
            name: "Birch ply 15".into(),
            thickness: 15.0,
            sheet_length: 2500.0,
            sheet_width: 1250.0,
            grain: true,
            color: "#e8d3a8".into(),
        },
        Material {
            id: "panel".into(),
            name: "Oak veneer MDF 6".into(),
            thickness: 6.0,
            sheet_length: 2800.0,
            sheet_width: 2070.0,
            grain: true,
            color: "#b98e60".into(),
        },
        Material {
            id: "top".into(),
            name: "Oak worktop 38".into(),
            thickness: 38.0,
            sheet_length: 4100.0,
            sheet_width: 650.0,
            grain: true,
            color: "#9c7350".into(),
        },
    ]
}

/// A small L-free kitchen run used as the starting project.
pub fn sample_kitchen() -> Project {
    let base = |id: &str, name: &str, w: f64| Cabinet {
        id: id.into(),
        name: name.into(),
        width: w,
        ..Cabinet::default()
    };
    let mut cabs = vec![
        Cabinet {
            id: "t1".into(),
            name: "Tall pantry".into(),
            kind: CabinetKind::Tall,
            width: 600.0,
            height: 2200.0,
            depth: 580.0,
            doors: 2,
            shelves: 5,
            ..Cabinet::default()
        },
        Cabinet {
            doors: 0,
            drawers: vec![180.0, 300.0, 290.0],
            shelves: 0,
            ..base("b1", "Drawer base", 600.0)
        },
        Cabinet {
            doors: 2,
            shelves: 0,
            ..base("b2", "Sink base", 800.0)
        },
        Cabinet {
            drawers: vec![160.0],
            ..base("b3", "Base", 450.0)
        },
    ];
    let mut walls = vec![
        Cabinet {
            id: "w1".into(),
            name: "Wall".into(),
            kind: CabinetKind::Wall,
            width: 600.0,
            height: 720.0,
            depth: 320.0,
            elevation: 1450.0,
            toe_kick_height: 0.0,
            shelves: 2,
            ..Cabinet::default()
        },
        Cabinet {
            id: "w2".into(),
            name: "Wall".into(),
            kind: CabinetKind::Wall,
            width: 800.0,
            height: 720.0,
            depth: 320.0,
            elevation: 1450.0,
            toe_kick_height: 0.0,
            doors: 2,
            shelves: 2,
            ..Cabinet::default()
        },
        Cabinet {
            id: "w3".into(),
            name: "Wall".into(),
            kind: CabinetKind::Wall,
            width: 450.0,
            height: 720.0,
            depth: 320.0,
            elevation: 1450.0,
            toe_kick_height: 0.0,
            hinge_side: HingeSide::Right,
            shelves: 2,
            ..Cabinet::default()
        },
    ];
    let mut x = 0.0;
    for c in cabs.iter_mut() {
        c.x = x;
        x += c.width;
    }
    let mut x = 600.0;
    for c in walls.iter_mut() {
        c.x = x;
        x += c.width;
    }
    cabs.append(&mut walls);
    for c in cabs.iter_mut() {
        c.door_style = DoorStyle::Shaker;
    }
    Project {
        name: "Kitchen".into(),
        settings: Settings::default(),
        materials: default_materials(),
        cabinets: cabs,
    }
}

impl Project {
    pub fn material(&self, id: &str) -> Material {
        self.materials
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .unwrap_or_else(|| Material {
                id: id.into(),
                name: id.into(),
                ..Material::default()
            })
    }
}
