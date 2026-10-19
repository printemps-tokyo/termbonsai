//! The tree's shape: every trunk step, branch step and leaf, with the day of
//! growth it appears on. Decided by the seed alone, so the same seed always
//! grows the same tree, and drawing it at a given growth just leaves out what
//! has not appeared yet.

use crate::rng::Rng;

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

/// Days between two trunk steps, and between two branch steps.
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
}

impl Grower {
    fn push(&mut self, x: i32, y: i32, ch: char, kind: Kind, born: f64) {
        let tag = self.rng.next_u64();
        self.parts.push(Part {
            x,
            y,
            ch,
            kind,
            born,
            tag,
        });
    }

    /// A flat cloud of leaves over a tip, the way bonsai foliage grows in
    /// pads, filling in over the days after `born`.
    fn leaves(&mut self, x: i32, y: i32, born: f64, size: i32) {
        let (rx, ry) = (2 * size + 1, size.max(1));
        for dy in 0..=ry {
            for dx in -rx..=rx {
                // Flatter on top: an ellipse cut at the tip's row.
                let d = (dx as f64 / rx as f64).powi(2) + (dy as f64 / (ry as f64 + 0.5)).powi(2);
                if d <= 1.0 && self.rng.f64() < 0.9 {
                    let b = born + 0.8 + d * 2.5 + self.rng.f64();
                    self.push(x + dx, y + dy, '&', Kind::Leaf, b);
                }
            }
        }
    }

    /// A branch from (x, y) going `side` (-1 or 1), and maybe its own branches.
    fn branch(&mut self, mut x: i32, mut y: i32, side: i32, born: f64, depth: u32) {
        let len = 3 + self.rng.below(3) as i32 + 6 / (depth as i32 + 2);
        let mut b = born;
        for step in 0..len {
            // Mostly outward, rising now and then: bonsai branches spread.
            let dy = if self.rng.f64() < 0.3 { 1 } else { 0 };
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

/// The whole tree for a seed.
pub fn grow(seed: u64) -> Vec<Part> {
    let mut g = Grower {
        rng: Rng::new(seed),
        parts: Vec::new(),
    };
    // The flare at the base.
    g.push(-1, 0, '/', Kind::Trunk, 0.0);
    g.push(1, 0, '\\', Kind::Trunk, 0.0);
    g.push(0, 0, '|', Kind::Trunk, 0.0);
    let height = 5 + g.rng.below(4) as i32;
    let (mut x, mut lean) = (0i32, 0i32);
    let mut side = if g.rng.f64() < 0.5 { 1 } else { -1 };
    let mut branches = 0;
    for step in 1..=height {
        // The trunk bends now and then, and straightens again.
        lean = if lean != 0 && g.rng.f64() < 0.6 {
            0
        } else if g.rng.f64() < 0.35 {
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
    g.leaves(x, height + 1, top + 2.0, 2);
    g.parts
}

/// The day of growth at which the tree is complete.
pub fn full_grown(parts: &[Part]) -> f64 {
    parts.iter().map(|p| p.born).fold(0.0, f64::max)
}
