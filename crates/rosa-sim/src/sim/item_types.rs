use glam::Vec3;
use rosa_protocol::clientbound::game::ItemKind;

use super::hull::ConvexHull;

// TODO: read from the second header vector of data/uzi.itm like load_item_file
const UZI_ITM_INERTIA: Vec3 = Vec3::new(f32::from_bits(0x3c7f2ac6), f32::from_bits(0x3c68311c), f32::from_bits(0x3b0be6ea));

pub struct ItemType {
    pub name: String,
    pub price: i32,
    pub mass: f32,
    pub can_collide: bool,
    pub bounds: Vec3,
    pub inv_inertia: Vec3,
    pub hull: Option<ConvexHull>,
    pub is_gun: bool,
    /// The one-handed guns: the holder aims across the body, turning the chest the other way (messedUpAiming).
    pub mirrored_aim: i32,
    pub hands: i32,
    pub magazine_ammo: i32,
    /// Which of the five pockets (inventory slots 2 to 6) the item fits in.
    pub pockets: [i32; 5],
    pub hold_pos: [Vec3; 2],
    pub hold_rot: [[f32; 4]; 2],
    /// Ticks between shots (+0x18).
    pub fire_rate: i32,
    /// Index into the bullet table (+0x1c).
    pub bullet_type: i32,
    // TODO: name once its readers are ported (+0x20, 90 for most guns)
    pub unk_20: i32,
    /// Muzzle speed per tick (+0x28).
    pub bullet_velocity: f32,
    /// How far a shot strays (+0x2c).
    pub bullet_spread: f32,
    /// Which item types this one mounts on (+0x11c), e.g. a magazine on its gun.
    pub can_mount_to: [i32; ItemKind::COUNT],
    /// Where a gun sits relative to its holder's right shoulder, along the hold matrix (+0x1394, gunHoldingPos).
    pub gun_hold_pos: Vec3,
}

impl ItemType {
    fn new(name: &str, price: i32, mass: f32, can_collide: bool, bounds: Vec3) -> Self {
        Self { name: name.to_string(), price, mass, can_collide, bounds, inv_inertia: inverse_inertia(bounds), hull: None, is_gun: false, mirrored_aim: 0, hands: 0, magazine_ammo: 0, pockets: [0; 5], hold_pos: [Vec3::ZERO; 2], hold_rot: [[0.0; 4]; 2], fire_rate: 0, bullet_type: 0, unk_20: 0, bullet_velocity: 0.0, bullet_spread: 0.0, can_mount_to: [0; ItemKind::COUNT], gun_hold_pos: Vec3::ZERO }
    }
}

fn inverse_inertia(b: Vec3) -> Vec3 {
    if !(b.x > 0.0 && b.y > 0.0 && b.z > 0.0) {
        return Vec3::ONE;
    }
    let (x2, y2, z2) = (b.x * b.x, b.y * b.y, b.z * b.z);
    Vec3::new(1.0 / ((y2 + z2) / 3.0), 1.0 / ((z2 + x2) / 3.0), 1.0 / ((y2 + x2) / 3.0))
}

pub fn item_types() -> Vec<ItemType> {
    use ItemKind::*;
    let v = Vec3::new;
    let rifle = v(0.03125, 3.0 / 64.0, 0.44);
    let long = v(0.03125, 9.0 / 128.0, 0.375);
    let mag = v(0.0625, 0.125, 0.25);
    let briefcase = v(0.3125, 0.125, 0.25);
    let cash = v(0.125, 0.0625, 0.25);
    let disk = v(0.25, 0.09375, 0.25);
    let phone = v(5.0 / 64.0, 15.0 / 64.0, 5.0 / 64.0);
    let paper = v(0.25, 0.125, 0.25);
    let furniture = v(0.5, 0.5, 0.5);
    let computer = v(0.25, 0.25, 0.25);
    let table = v(1.0, 0.5, 0.5);
    let tall = v(0.125, 0.25, 0.125);

    let defs: [(ItemKind, &str, i32, f32, bool, Vec3); ItemKind::COUNT] = [
        (Auto5, "Auto 5", 500, 3.5, false, rifle),
        (Ak47, "AK-47", 500, 5.0, false, rifle),
        (Ak47Mag, "AK-47 Magazine", 50, 0.75, false, mag),
        (M16, "M-16", 750, 3.25, false, long),
        (M16Mag, "M-16 Magazine", 75, 0.75, false, mag),
        (Magnum, "Magnum", 10000, 1.25, false, v(0.0625, 0.09375, 0.1875)),
        (MagnumMag, ".45 Bullets", 1000, 0.5, false, mag),
        (Mp5, "MP5", 300, 2.5, false, long),
        (Mp5Mag, "MP5 Magazine", 30, 0.625, false, mag),
        (Uzi, "Uzi", 1000, 2.5, false, v(0.0625, 0.09375, 0.3125)),
        (UziMag, "Uzi Magazine", 100, 0.625, false, v(0.0625, 0.125, 0.125)),
        (Pistol, "9mm", 50, 1.0, false, v(0.125, 0.1875, 0.375)),
        (PistolMag, "9mm Magazine", 5, 0.5, false, mag),
        (Grenade, "Grenade", 1000, 0.75, false, v(0.0625, 0.0625, 0.0625)),
        (Bandage, "Bandage", 10, 1.0, false, v(0.25, 0.125, 0.25)),
        (Briefcase, "Briefcase", 50, 4.0, false, briefcase),
        (BriefcaseOpen, "Open Briefcase", 0, 4.0, false, briefcase),
        (CashRound, "Cash", 0, 0.1, false, cash),
        (CashWorld, "Cash", 0, 0.1, false, cash),
        (DiskBlack, "Black Disk", 20, 1.0, false, disk),
        (DiskGreen, "Green Disk", 0, 1.0, false, disk),
        (DiskBlue, "Blue Disk", 0, 1.0, false, disk),
        (DiskWhite, "White Disk", 0, 1.0, false, disk),
        (DiskGold, "Gold Disk", 0, 1.0, false, disk),
        (DiskRed, "Red Disk", 0, 1.0, false, disk),
        (Phone, "Cell Phone", 0, 1.0, false, phone),
        (Radio, "Walkie-talkie", 100, 0.75, false, v(3.0 / 64.0, 9.0 / 64.0, 3.0 / 64.0)),
        (Key, "Key", 0, 1.0, false, v(0.0625, 0.0625, 0.125)),
        (Door, "Door", 0, 20.0, true, v(0.4375, 1.125, 0.0625)),
        (PaperWorld, "Newspaper", 0, 0.5, false, paper),
        (Burger, "Burger", 0, 1.0, false, v(0.125, 0.125, 0.125)),
        (Desk, "Desk", 0, 16.0, false, furniture),
        (Lamp, "Lamp", 0, 16.0, false, furniture),
        (PhonePay, "Pay Phone", 0, 1.0, false, phone),
        (Paper, "Memo", 0, 0.5, false, paper),
        (SoccerBall, "Soccer Ball", 100, 0.45, false, v(0.11, 0.11, 0.11)),
        (Rope, "Rope", 100, 2.0, false, v(0.0625, 0.25, 0.0625)),
        (Box, "Box", 0, 20.0, true, computer),
        (BigBox, "Big Box", 0, 20.0, true, v(1.0, 0.125, 0.5)),
        (Computer, "computer", 0, 40.0, true, computer),
        (Arcade, "arcade", 0, 40.0, true, computer),
        (Table, "Table", 0, 320.0, true, table),
        (TableTest, "tabletest2", 0, 40.0, true, table),
        (Wall, "cubewall", 0, 80.0, true, v(1.75, 0.75, 0.125)),
        (Bottle, "Bottle", 0, 4.0, true, tall),
        (Watermelon, "Watermelon", 10, 4.0, true, tall),
    ];

    let mut types: Vec<ItemType> = defs
        .iter()
        .enumerate()
        .map(|(i, &(kind, name, price, mass, can_collide, bounds))| {
            debug_assert_eq!(kind as usize, i);
            ItemType::new(name, price, mass, can_collide, bounds)
        })
        .collect();

    for (t, (is_gun, mirrored_aim, hands, hold_pos, hold_rot)) in types.iter_mut().zip(hold_data()) {
        t.is_gun = is_gun != 0;
        t.mirrored_aim = mirrored_aim;
        t.hands = hands;
        t.hold_pos = hold_pos;
        t.hold_rot = hold_rot;
    }
    for (t, ammo) in types.iter_mut().zip(MAGAZINE_AMMO) {
        t.magazine_ammo = ammo;
    }
    for (k, t) in types.iter_mut().enumerate() {
        t.fire_rate = FIRE_RATE[k];
        t.bullet_type = BULLET_TYPE[k];
        t.unk_20 = UNK_20[k];
        t.bullet_velocity = BULLET_VELOCITY[k];
        t.bullet_spread = BULLET_SPREAD[k];
        t.gun_hold_pos = GUN_HOLD_POS[k];
    }
    for (kind, onto) in MOUNTS {
        for &o in onto {
            types[kind as usize].can_mount_to[o as usize] = 1;
        }
    }
    for (t, pockets) in types.iter_mut().zip(POCKETS) {
        t.pockets = pockets;
    }

    let ball = &mut types[SoccerBall as usize];
    let r = ball.bounds.x;
    ball.inv_inertia = Vec3::splat(1.0 / (r * r * 0.45));

    for kind in [Auto5, Uzi] {
        types[kind as usize].inv_inertia = Vec3::ONE / UZI_ITM_INERTIA;
    }

    for (i, t) in types.iter_mut().enumerate() {
        if !t.can_collide || i == TableTest as usize {
            continue;
        }
        t.hull = Some(match i {
            i if i == Bottle as usize => ConvexHull::prism(0.125, 0.09375, 0.5, false),
            i if i == Watermelon as usize => ConvexHull::prism(0.1875, 0.09375, 0.6, true),
            _ => ConvexHull::bounding_box(t.bounds),
        });
    }
    for (kind, file) in IT3_MESHES {
        let path = std::path::Path::new("data").join("item").join(file);
        match rosa_map::file_types::it3::It3File::load(&path) {
            Ok(it3) => {
                if let Some(c) = it3.collision {
                    types[kind as usize].hull = Some(ConvexHull::from_it3(&c));
                }
            }
            Err(e) => eprintln!("[Items] could not load {}: {e}", path.display()),
        }
    }
    
    types
}

/// The item types whose collision hull comes from an .it3 mesh (load_it3_file).
const IT3_MESHES: [(ItemKind, &str); 4] = [(ItemKind::Computer, "computer.it3"), (ItemKind::Arcade, "computer.it3"), (ItemKind::TableTest, "tabletest2.it3"), (ItemKind::Wall, "cubewall.it3")];

const FIRE_RATE: [i32; ItemKind::COUNT] = [6, 8, 0, 7, 0, 30, 0, 5, 0, 6, 0, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const BULLET_TYPE: [i32; ItemKind::COUNT] = [2, 0, 0, 1, 0, 3, 0, 2, 0, 2, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const UNK_20: [i32; ItemKind::COUNT] = [90, 90, 0, 90, 0, 18, 0, 90, 0, 90, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const BULLET_VELOCITY: [f32; ItemKind::COUNT] = [f32::from_bits(0x40d55555), f32::from_bits(0x412aaaab), f32::from_bits(0x0), f32::from_bits(0x416aaaab), f32::from_bits(0x0), f32::from_bits(0x40f55555), f32::from_bits(0x0), f32::from_bits(0x40d55555), f32::from_bits(0x0), f32::from_bits(0x40d55555), f32::from_bits(0x0), f32::from_bits(0x40d55555), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0)];
const BULLET_SPREAD: [f32; ItemKind::COUNT] = [f32::from_bits(0x3daaaaab), f32::from_bits(0x3daaaaab), f32::from_bits(0x0), f32::from_bits(0x3d4ccccd), f32::from_bits(0x0), f32::from_bits(0x3d088889), f32::from_bits(0x0), f32::from_bits(0x3d088889), f32::from_bits(0x0), f32::from_bits(0x3d4ccccd), f32::from_bits(0x0), f32::from_bits(0x3d088889), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0), f32::from_bits(0x0)];
const GUN_HOLD_POS: [Vec3; ItemKind::COUNT] = {
    let mut p = [Vec3::ZERO; ItemKind::COUNT];
    p[ItemKind::Ak47 as usize] = Vec3::new(0.0, -0.0625, 0.375);
    p[ItemKind::M16 as usize] = Vec3::new(0.0, -0.03125, 0.375);
    p[ItemKind::Mp5 as usize] = Vec3::new(0.0, -0.0625, 0.375);
    p[ItemKind::Uzi as usize] = Vec3::new(0.0, -0.0625, 0.375);
    p
};
const MOUNTS: [(ItemKind, &[ItemKind]); 14] = [
    (ItemKind::Ak47Mag, &[ItemKind::Ak47]),
    (ItemKind::M16Mag, &[ItemKind::M16]),
    (ItemKind::MagnumMag, &[ItemKind::Magnum]),
    (ItemKind::Mp5Mag, &[ItemKind::Mp5]),
    (ItemKind::UziMag, &[ItemKind::Uzi]),
    (ItemKind::PistolMag, &[ItemKind::Pistol]),
    (ItemKind::CashRound, &[ItemKind::BriefcaseOpen]),
    (ItemKind::CashWorld, &[ItemKind::BriefcaseOpen]),
    (ItemKind::DiskBlack, &[ItemKind::BriefcaseOpen, ItemKind::Computer]),
    (ItemKind::DiskGreen, &[ItemKind::BriefcaseOpen, ItemKind::Computer]),
    (ItemKind::DiskBlue, &[ItemKind::BriefcaseOpen, ItemKind::Computer]),
    (ItemKind::DiskWhite, &[ItemKind::BriefcaseOpen, ItemKind::Computer]),
    (ItemKind::DiskGold, &[ItemKind::BriefcaseOpen, ItemKind::Computer]),
    (ItemKind::DiskRed, &[ItemKind::BriefcaseOpen, ItemKind::Computer]),
];

const MAGAZINE_AMMO: [i32; ItemKind::COUNT] = [
    300, 0, 30, 0, 30, 0, 6, 0, 30, 0, 30, 0, 12, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const POCKETS: [[i32; 5]; ItemKind::COUNT] = [
    [0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [1, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [1, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [1, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [1, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [1, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 1, 1, 1, 1],
    [0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
];

/// The gun flags, hand count and per-hand hold position and rotation (axis, angle) of each type, as item_types_init
/// fills them in.
#[allow(clippy::type_complexity)]
fn hold_data() -> [(i32, i32, i32, [Vec3; 2], [[f32; 4]; 2]); ItemKind::COUNT] {
    let v = Vec3::new;
    [
        (1, 0, 2, [v(1.237418e-9, -0.06208252, 0.03192755), v(-4.274715e-9, -0.017045714, -0.09417537)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.99999994, 0.0, 0.0, 90.0_f32.to_radians()]]),
        (1, 0, 2, [v(0.0, -0.09375, 0.0625), v(0.0, -0.0625, -0.125)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.99999994, 0.0, 0.0, 90.0_f32.to_radians()]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (1, 0, 2, [v(0.0, -0.09375, 0.0625), v(0.0, -0.0625, -0.125)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.99999994, 0.0, 0.0, 90.0_f32.to_radians()]]),
        (0, 0, 0, [v(0.0, -0.0625, -0.09375), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (1, 1, 0, [v(0.0, -0.125, 0.0625), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (1, 0, 2, [v(0.0, -0.09375, 0.0625), v(0.0, -0.0625, -0.125)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.99999994, 0.0, 0.0, 90.0_f32.to_radians()]]),
        (0, 0, 0, [v(0.0, -0.0625, -0.09375), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (1, 0, 2, [v(1.237418e-9, -0.06208252, 0.03192755), v(-4.274715e-9, -0.017045714, -0.09417537)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.99999994, 0.0, 0.0, 90.0_f32.to_radians()]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (1, 1, 1, [v(0.0, -0.0625, -0.03125), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 1, [v(0.0, 0.0, 0.25), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 1, [v(0.0, -0.125, 0.25), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.125), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.4375, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[-0.57735026, -0.57735026, 0.57735026, 240.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.1875, 0.0, 0.1875), v(-0.1875, 0.0, 0.1875)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0625), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.1875, 0.0, 0.1875), v(-0.1875, 0.0, 0.1875)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.99999994, 0.0, 0.0, 90.0_f32.to_radians()]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.99999994, 0.0, 0.0, 90.0_f32.to_radians()], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
        (0, 0, 0, [v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)], [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]),
    ]
}
