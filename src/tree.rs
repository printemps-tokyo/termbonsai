//! The tree's shape: every trunk step, branch step and leaf, with the day of
//! growth it appears on. Decided by the seed alone, so the same seed always
//! grows the same tree, and drawing it at a given growth just leaves out what
//! has not appeared yet.

use crate::rng::Rng;

/// The kind of tree. `Classic` is the tree termbonsai has always grown; the
/// others change its proportions here and its leaves in `draw`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Species {
    Classic,
    /// Black pine: a taller, bent trunk, long flat branches, evergreen pads.
    Pine,
    /// Japanese maple: red in autumn, bare in winter.
    Maple,
    /// Cherry: wide, rising branches, covered in blossom in spring.
    Cherry,
}

impl Species {
    pub const ALL: [Species; 4] = [
        Species::Classic,
        Species::Pine,
        Species::Maple,
        Species::Cherry,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Species::Classic => "classic",
            Species::Pine => "pine",
            Species::Maple => "maple",
            Species::Cherry => "cherry",
        }
    }

    pub fn parse(s: &str) -> Option<Species> {
        Species::ALL.into_iter().find(|sp| sp.id() == s)
    }

    /// The proportions this species grows with.
    fn shape(self) -> Shape {
        match self {
            Species::Classic => Shape::CLASSIC,
            Species::Pine => Shape {
                min_height: 6,
                rise: 0.12,
                lean: 0.5,
                branch_extra: 2,
                flat_pads: true,
                ..Shape::CLASSIC
            },
            Species::Maple => Shape {
                rise: 0.35,
                fill: 0.95,
                ..Shape::CLASSIC
            },
            Species::Cherry => Shape {
                height_spread: 3,
                rise: 0.45,
                branch_extra: 1,
                crown: 3,
                ..Shape::CLASSIC
            },
        }
    }
}

/// Numbers that set a tree's proportions. `CLASSIC` holds the values the
/// classic tree has always used, so it grows exactly as before.
#[derive(Clone, Copy)]
struct Shape {
    /// Trunk height: `min_height` + a draw below `height_spread`.
    min_height: i32,
    height_spread: u64,
    /// Chance that a branch step also rises a row.
    rise: f64,
    /// Chance that a straight trunk starts to lean.
    lean: f64,
    /// Extra steps on every branch.
    branch_extra: i32,
    /// Chance that a cell inside a leaf pad has a leaf.
    fill: f64,
    /// Pads one row deep, as pine foliage grows.
    flat_pads: bool,
    /// Size of the pad on top of the trunk.
    crown: i32,
}

impl Shape {
    const CLASSIC: Shape = Shape {
        min_height: 5,
        height_spread: 4,
        rise: 0.3,
        lean: 0.35,
        branch_extra: 0,
        fill: 0.9,
        flat_pads: false,
        crown: 2,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Trunk,
    Branch,
    Leaf,
}

/// One character of the tree. `y` grows upward from the base at 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub x: i32,
    pub y: i32,
    pub ch: char,
    pub kind: Kind,
    /// Days of growth after which it is there.
    pub born: f64,
    /// A number fixed per part, for choices made when drawing (which leaves
    /// fall first, which turn red).
    pub tag: u64,
}

/// Stretches every appearance time, so a tree takes about a month of wet
/// soil to fill out (between about 27 and 46 days, by seed).
pub const GROWTH_SCALE: f64 = 2.5;

/// Days between two trunk steps, and between two branch steps (before
/// `GROWTH_SCALE`).
const TRUNK_STEP: f64 = 0.6;
const BRANCH_STEP: f64 = 0.45;

fn stroke(dx: i32, dy: i32) -> char {
    match (dx, dy) {
        (0, _) => '|',
        (1, 1) => '/',
        (-1, 1) => '\\',
        _ => '_',
    }
}

struct Grower {
    rng: Rng,
    parts: Vec<Part>,
    shape: Shape,
}

impl Grower {
    fn push(&mut self, x: i32, y: i32, ch: char, kind: Kind, born: f64) {
        let tag = self.rng.next_u64();
        self.parts.push(Part {
            x,
            y,
            ch,
            kind,
            born: born * GROWTH_SCALE,
            tag,
        });
    }

    /// A flat cloud of leaves over a tip, the way bonsai foliage grows in
    /// pads, filling in over the days after `born`.
    fn leaves(&mut self, x: i32, y: i32, born: f64, size: i32) {
        let (rx, ry) = (
            2 * size + 1,
            if self.shape.flat_pads { 1 } else { size.max(1) },
        );
        for dy in 0..=ry {
            for dx in -rx..=rx {
                // Flatter on top: an ellipse cut at the tip's row.
                let d = (dx as f64 / rx as f64).powi(2) + (dy as f64 / (ry as f64 + 0.5)).powi(2);
                if d <= 1.0 && self.rng.f64() < self.shape.fill {
                    let b = born + 0.8 + d * 2.5 + self.rng.f64();
                    self.push(x + dx, y + dy, '&', Kind::Leaf, b);
                }
            }
        }
    }

    /// A branch from (x, y) going `side` (-1 or 1), and maybe its own branches.
    fn branch(&mut self, mut x: i32, mut y: i32, side: i32, born: f64, depth: u32) {
        let len = 3 + self.rng.below(3) as i32 + 6 / (depth as i32 + 2) + self.shape.branch_extra;
        let mut b = born;
        for step in 0..len {
            // Mostly outward, rising now and then: bonsai branches spread.
            let dy = if self.rng.f64() < self.shape.rise {
                1
            } else {
                0
            };
            x += side;
            y += dy;
            b += BRANCH_STEP;
            self.push(x, y, stroke(side, dy), Kind::Branch, b);
            if depth < 1 && step >= 2 && step < len - 1 && self.rng.f64() < 0.3 {
                self.branch(x, y + 1, side, b + 1.5, depth + 1);
            }
        }
        let size = 2 - depth.min(1) as i32;
        self.leaves(x, y + 1, b, size);
    }
}

/// The whole classic tree for a seed.
pub fn grow(seed: u64) -> Vec<Part> {
    grow_species(seed, Species::Classic)
}

/// The whole tree of a species for a seed.
pub fn grow_species(seed: u64, species: Species) -> Vec<Part> {
    let shape = species.shape();
    let mut g = Grower {
        rng: Rng::new(seed),
        parts: Vec::new(),
        shape,
    };
    // The flare at the base.
    g.push(-1, 0, '/', Kind::Trunk, 0.0);
    g.push(1, 0, '\\', Kind::Trunk, 0.0);
    g.push(0, 0, '|', Kind::Trunk, 0.0);
    let height = shape.min_height + g.rng.below(shape.height_spread) as i32;
    let (mut x, mut lean) = (0i32, 0i32);
    let mut side = if g.rng.f64() < 0.5 { 1 } else { -1 };
    let mut branches = 0;
    for step in 1..=height {
        // The trunk bends now and then, and straightens again.
        lean = if lean != 0 && g.rng.f64() < 0.6 {
            0
        } else if g.rng.f64() < shape.lean {
            if g.rng.f64() < 0.5 {
                1
            } else {
                -1
            }
        } else {
            lean
        };
        let dx = if step > 1 { lean } else { 0 };
        x += dx;
        let b = step as f64 * TRUNK_STEP;
        g.push(x, step, stroke(dx, 1), Kind::Trunk, b);
        // Branches on alternating sides, at least two of them, so no tree
        // ends up a bare pole.
        let must = branches < 2 && step >= height - (2 - branches);
        if step >= 2 && step < height && (must || g.rng.f64() < 0.7) {
            g.branch(x, step, side, b + 2.0, 0);
            side = -side;
            branches += 1;
        }
    }
    let top = height as f64 * TRUNK_STEP;
    g.leaves(x, height + 1, top + 2.0, shape.crown);
    g.parts
}

/// The day of growth at which the tree is complete.
pub fn full_grown(parts: &[Part]) -> f64 {
    parts.iter().map(|p| p.born).fold(0.0, f64::max)
}
