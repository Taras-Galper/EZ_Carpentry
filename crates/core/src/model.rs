//! Project data model. Everything is in millimetres.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub name: String,
    pub settings: Settings,
    pub materials: Vec<Material>,
    pub cabinets: Vec<Cabinet>,
    pub pricing: Pricing,
    pub room: crate::room::Room,
    pub cnc: crate::export::gcode::Cnc,
    /// Who the job is for (printed on the customer quote).
    pub client: Contact,
}

/// Name and contact lines for the customer quote.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Contact {
    pub name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
}

/// The shop, as shown on customer quotes (kept with the shop defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Company {
    pub name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    /// Logo as a data: URL (PNG / JPEG / SVG); empty for none.
    pub logo: String,
    /// Payment terms, lead time and so on; one paragraph per line.
    pub terms: String,
    /// Days the quote stays valid.
    pub valid_days: u32,
}

impl Default for Company {
    fn default() -> Self {
        Self {
            name: String::new(),
            phone: String::new(),
            email: String::new(),
            address: String::new(),
            logo: String::new(),
            terms: "50% deposit on order, balance on installation.\nPrices include supply and installation.".into(),
            valid_days: 30,
        }
    }
}

/// Prices for quoting. Currency is just a label.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Pricing {
    pub currency: String,
    /// Per metre of edge band.
    pub edge_band: f64,
    /// Per labour hour.
    pub labor_rate: f64,
    /// CNC / saw time per sheet, minutes.
    pub cnc_minutes_per_sheet: f64,
    /// Assembly time per cabinet, hours.
    pub assembly_hours: f64,
    /// Installation time per cabinet, hours.
    pub install_hours: f64,
    pub markup_pct: f64,
    pub vat_pct: f64,
    /// Unit price by hardware item name; missing items use a default.
    pub hardware: BTreeMap<String, f64>,
    /// Supplier order code by hardware item name (shown on lists).
    pub codes: BTreeMap<String, String>,
    pub company: Company,
}

impl Default for Pricing {
    fn default() -> Self {
        Self {
            currency: "₪".into(),
            edge_band: 4.0,
            labor_rate: 150.0,
            cnc_minutes_per_sheet: 20.0,
            assembly_hours: 1.5,
            install_hours: 0.75,
            markup_pct: 30.0,
            vat_pct: 18.0,
            hardware: BTreeMap::new(),
            codes: BTreeMap::new(),
            company: Company::default(),
        }
    }
}

impl Pricing {
    /// Unit price for a hardware item: explicit price, else a default by kind.
    pub fn hardware_price(&self, name: &str) -> f64 {
        if let Some(p) = self.hardware.get(name) {
            return *p;
        }
        let n = name.to_lowercase();
        if n.starts_with("handle") {
            25.0
        } else if n.contains("hinge") {
            15.0
        } else if n.contains("slide") {
            90.0
        } else if n.contains("shelf pin") {
            0.5
        } else {
            0.0
        }
    }
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
    /// Worktop overhang past a free-standing run end (beyond any end panel).
    pub worktop_end_overhang: f64,
    /// Shaker stile/rail width.
    pub shaker_frame: f64,
    /// Shaker panel groove depth (also the rail tenon length).
    pub shaker_groove: f64,
    /// Depth of the recess routed into one-piece Shaker fronts.
    pub routed_depth: f64,
    pub joinery: Joinery,
    pub back_mode: BackMode,
    pub plinth: Plinth,
    /// One plinth board per run of floor cabinets (with returns at free
    /// ends) instead of a toe kick under each cabinet.
    pub plinth_runs: bool,
    /// Grooved back: distance from the carcass back edge to the groove.
    pub back_inset: f64,
    /// Depth of back-panel and drawer-bottom grooves.
    pub groove_depth: f64,
    /// Legacy rectangular room (read from older files, moved into
    /// `Project::room` on load).
    pub room_width: f64,
    pub room_depth: f64,
    pub room_height: f64,
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
            worktop_end_overhang: 0.0,
            plinth_runs: true,
            shaker_frame: 60.0,
            shaker_groove: 8.0,
            routed_depth: 6.0,
            joinery: Joinery::Confirmat,
            back_mode: BackMode::Applied,
            plinth: Plinth::Board,
            back_inset: 10.0,
            groove_depth: 8.0,
            room_width: 0.0,
            room_depth: 0.0,
            room_height: 0.0,
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
    /// Price per sheet (or worktop blank).
    pub price: f64,
    /// Offcuts in stock, [length, width]: nested into before new sheets,
    /// and not charged.
    pub offcuts: Vec<[f64; 2]>,
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
            price: 350.0,
            offcuts: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CabinetKind {
    Base,
    Wall,
    Tall,
    /// Front-material strip closing a gap (plus toe kick when on the floor).
    Filler,
    /// Integrated dishwasher: decor panel and plinth only.
    Dishwasher,
    /// Space held for a fridge, freestanding oven, etc. No parts.
    Appliance,
}

impl CabinetKind {
    /// Built as a box with sides, top, bottom and back.
    pub fn is_carcass(self) -> bool {
        matches!(self, CabinetKind::Base | CabinetKind::Wall | CabinetKind::Tall)
    }
}

impl Cabinet {
    /// Stands on the floor on a plinth that joins its neighbours' (run plinths).
    pub fn on_run_plinth(&self) -> bool {
        self.elevation == 0.0
            && self.toe_kick_height > 0.0
            && !self.is_l()
            && matches!(self.kind, CabinetKind::Base | CabinetKind::Tall | CabinetKind::Dishwasher | CabinetKind::Filler)
    }

    pub fn is_l(&self) -> bool {
        self.corner == Corner::L && self.kind.is_carcass()
    }

    /// Plan footprint as local rectangles [x0, z0, x1, z1].
    pub fn rects(&self) -> Vec<[f64; 4]> {
        let (w, d) = (self.width, self.depth);
        if self.is_l() {
            vec![[0.0, 0.0, w, d], [(w - d).max(0.0), 0.0, w, self.width2.max(d)]]
        } else {
            vec![[0.0, 0.0, w, d]]
        }
    }

    /// Sits on the floor under the worktop (joins worktop runs).
    pub fn under_worktop(&self) -> bool {
        match self.kind {
            CabinetKind::Base | CabinetKind::Dishwasher => true,
            CabinetKind::Filler | CabinetKind::Appliance => self.elevation == 0.0 && self.height < 1000.0,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoorStyle {
    #[default]
    Slab,
    /// Five-piece frame: two stiles, two rails, a recessed panel.
    Shaker,
    /// One-piece MDF front with the Shaker recess routed into its face.
    ShakerRouted,
}

/// How carcass panels are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Joinery {
    /// Ø7 through the side, Ø5 pilot into the panel edge.
    #[default]
    Confirmat,
    /// Ø8 blind in the side, Ø8 into the panel edge.
    Dowel,
}

/// What the floor-standing carcasses stand on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Plinth {
    /// Sides run to the floor with a toe-kick notch; board fixed to the carcass.
    #[default]
    Board,
    /// Carcass on adjustable legs; plinth board clips to the legs.
    Legs,
}

/// How the back panel is fitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackMode {
    /// Fixed over the back edges of the carcass.
    #[default]
    Applied,
    /// Slid into grooves in the sides, bottom and top.
    Grooved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CutoutKind {
    #[default]
    None,
    Sink,
    Hob,
}

impl CutoutKind {
    /// Default cut-out (width, depth, corner radius) when none is given.
    pub fn default_size(self) -> (f64, f64, f64) {
        match self {
            CutoutKind::Sink => (760.0, 440.0, 10.0),
            CutoutKind::Hob => (560.0, 490.0, 5.0),
            CutoutKind::None => (0.0, 0.0, 0.0),
        }
    }
}

/// Corner cabinet shape. `L`: a second arm runs from the back-right
/// corner toward the front along local z, `width2` long overall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    #[default]
    None,
    L,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlindSide {
    #[default]
    None,
    Left,
    Right,
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
    /// Doors are frames with glass (bought in, listed with the hardware).
    pub glass: bool,
    /// Wall cabinet: one full-width front on a lift-up stay fitting.
    pub lift_up: bool,
    /// Blind corner: this side of the front is a fixed panel that the
    /// perpendicular run covers; the door fills the rest.
    pub blind_side: BlindSide,
    pub blind_width: f64,
    /// L-shaped corner unit: second arm along local z, `width2` overall.
    pub corner: Corner,
    pub width2: f64,
    /// Finished end panels (front material) outside the carcass sides.
    pub end_left: bool,
    pub end_right: bool,
    /// Finished panel (front material) over the back, for islands.
    pub back_finished: bool,
    /// Wardrobe: fixed hat shelf near the top with a hanging rail under it.
    pub hanging_rail: bool,
    /// Gap between the hat shelf and the top panel.
    pub hat_space: f64,
    /// Appliance housing: open slot of this height (0 = none) whose bottom
    /// is `housing_elevation` above the floor, between fixed shelves.
    pub housing_height: f64,
    pub housing_elevation: f64,
    /// Worktop cut-out centred over this cabinet. Sizes of 0 use the
    /// kind's default; setback is from the worktop front edge.
    pub cutout: CutoutKind,
    pub cutout_width: f64,
    pub cutout_depth: f64,
    pub cutout_radius: f64,
    pub cutout_setback: f64,
    /// Worktop overhang behind the cabinet (seating at an island or
    /// peninsula); the largest in a run applies.
    pub worktop_back: f64,
    /// Cabinets with the same group move and rotate together (islands).
    pub group: String,
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
            glass: false,
            lift_up: false,
            blind_side: BlindSide::None,
            blind_width: 600.0,
            corner: Corner::None,
            width2: 900.0,
            end_left: false,
            end_right: false,
            back_finished: false,
            hanging_rail: false,
            hat_space: 300.0,
            housing_height: 0.0,
            housing_elevation: 850.0,
            cutout: CutoutKind::None,
            cutout_width: 0.0,
            cutout_depth: 0.0,
            cutout_radius: 0.0,
            cutout_setback: 60.0,
            worktop_back: 0.0,
            group: String::new(),
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
            price: 120.0,
            offcuts: vec![],
        },
        Material {
            id: "front".into(),
            name: "Oak veneer MDF 19".into(),
            thickness: 19.0,
            sheet_length: 2800.0,
            sheet_width: 2070.0,
            grain: true,
            color: "#c49a6c".into(),
            price: 900.0,
            offcuts: vec![],
        },
        Material {
            id: "drawer".into(),
            name: "Birch ply 15".into(),
            thickness: 15.0,
            sheet_length: 2500.0,
            sheet_width: 1250.0,
            grain: true,
            color: "#e8d3a8".into(),
            price: 380.0,
            offcuts: vec![],
        },
        Material {
            id: "panel".into(),
            name: "Oak veneer MDF 6".into(),
            thickness: 6.0,
            sheet_length: 2800.0,
            sheet_width: 2070.0,
            grain: true,
            color: "#b98e60".into(),
            price: 450.0,
            offcuts: vec![],
        },
        Material {
            id: "top".into(),
            name: "Oak worktop 38".into(),
            thickness: 38.0,
            sheet_length: 4100.0,
            sheet_width: 650.0,
            grain: true,
            color: "#9c7350".into(),
            price: 1600.0,
            offcuts: vec![],
        },
    ]
}

/// L-shaped sample kitchen: tall + drawers + sink + blind corner along
/// the back wall, a return leg along the right wall, wall units above.
pub fn sample_kitchen() -> Project {
    let base = |id: &str, name: &str, w: f64| Cabinet { id: id.into(), name: name.into(), width: w, ..Cabinet::default() };
    let wall = |id: &str, w: f64| Cabinet {
        id: id.into(),
        name: "Wall".into(),
        kind: CabinetKind::Wall,
        width: w,
        height: 720.0,
        depth: 320.0,
        elevation: 1450.0,
        toe_kick_height: 0.0,
        shelves: 2,
        ..Cabinet::default()
    };
    let room_w = 3200.0;
    let s = Settings::default();
    let mut room = crate::room::Room::rectangle(room_w, 2600.0);
    // Window over the sink, door on the left wall.
    room.openings = vec![
        crate::room::Opening {
            wall: 0,
            kind: crate::room::OpeningKind::Window,
            offset: 1300.0,
            width: 700.0,
            height: 1150.0,
            sill: 1000.0,
        },
        crate::room::Opening { wall: 3, offset: 900.0, width: 850.0, ..Default::default() },
    ];
    let top_depth = 560.0 + s.worktop_overhang;
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
        Cabinet { kind: CabinetKind::Dishwasher, doors: 0, shelves: 0, ..base("d1", "Dishwasher", 600.0) },
        Cabinet { doors: 2, shelves: 0, cutout: CutoutKind::Sink, ..base("b2", "Sink base", 800.0) },
        Cabinet { blind_side: BlindSide::Right, blind_width: top_depth + 20.0, drawers: vec![], ..base("b3", "Blind corner", 1150.0) },
    ];
    // 50 mm filler at the left wall so the pantry's left door can open fully.
    cabs.insert(
        0,
        Cabinet {
            id: "f1".into(),
            name: "Filler".into(),
            kind: CabinetKind::Filler,
            width: 50.0,
            height: 2200.0,
            depth: 580.0,
            doors: 0,
            shelves: 0,
            ..Cabinet::default()
        },
    );
    let mut x = 0.0;
    for c in cabs.iter_mut() {
        c.x = x;
        x += c.width;
    }
    // Return leg on the right wall, facing -x; starts where the back worktop ends.
    let mut z = top_depth;
    for c in [
        Cabinet { drawers: vec![160.0], ..base("b4", "Base", 450.0) },
        Cabinet { doors: 0, drawers: vec![180.0, 300.0, 290.0], shelves: 0, cutout: CutoutKind::Hob, ..base("b5", "Hob base", 600.0) },
        Cabinet { drawers: vec![160.0], hinge_side: HingeSide::Right, end_right: true, ..base("b6", "Base", 450.0) },
    ] {
        cabs.push(Cabinet { x: room_w, z, rotation: -90.0, ..c });
        z += cabs.last().map(|c| c.width).unwrap_or(0.0);
    }
    // Wall cabinets either side of the window.
    for (id, x, w) in [("w1", 650.0, 600.0), ("w3", 2050.0, 600.0)] {
        cabs.push(Cabinet { x, ..wall(id, w) });
    }
    if let Some(w3) = cabs.iter_mut().find(|c| c.id == "w3") {
        w3.hinge_side = HingeSide::Right;
    }
    for c in cabs.iter_mut() {
        c.door_style = DoorStyle::Shaker;
    }
    Project {
        name: "Kitchen".into(),
        settings: s,
        materials: default_materials(),
        cabinets: cabs,
        pricing: Pricing::default(),
        room,
        cnc: Default::default(),
        client: Contact::default(),
    }
}

impl Project {
    /// Bring older files up to date after parsing.
    pub fn migrate(&mut self) {
        let s = &mut self.settings;
        if self.room.walls.is_empty() && s.room_width > 0.0 {
            let d = if s.room_depth > 0.0 { s.room_depth } else { 3000.0 };
            self.room = crate::room::Room::rectangle(s.room_width, d);
            if s.room_height > 0.0 {
                self.room.height = s.room_height;
            }
        }
        s.room_width = 0.0;
        s.room_depth = 0.0;
        s.room_height = 0.0;
        self.room.orient();
    }

    pub fn material(&self, id: &str) -> Material {
        self.materials.iter().find(|m| m.id == id).cloned().unwrap_or_else(|| Material {
            id: id.into(),
            name: id.into(),
            ..Material::default()
        })
    }
}
