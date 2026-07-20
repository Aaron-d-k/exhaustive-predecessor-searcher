use crate::basic_grids::{Grid, PackedCells, PackedCellsUnderlying, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CACell {
    DEAD,
    ALIVE,
}

impl Into<u8> for CACell {
    fn into(self) -> u8 {
        match self {
            CACell::DEAD => 0,
            CACell::ALIVE => 1,
        }
    }
}

impl PackedCells {
    #[allow(dead_code)]
    pub fn to_grid(&self, r: Rect) -> Grid<CACell> {
        let mut g = Grid::new(r.height(), r.width(), CACell::DEAD);
        for y in 0..r.height() {
            for x in 0..r.width() {
                *g.get_mut((x, y).into()) = if (self.0 >> (y * r.width() + x)) & 1 == 1 {
                    CACell::ALIVE
                } else {
                    CACell::DEAD
                };
            }
        }
        g
    }
}

pub struct RuleLut {
    zeropred: Vec<PackedCells>,
    onepred: Vec<PackedCells>,
    forward_lut: [CACell; 512],
}

impl RuleLut {
    pub fn new(rule_str: &str) -> Self {
        let mut b_rules = [false; 9];
        let mut s_rules = [false; 9];

        let mut parsing_b = false;
        let mut parsing_s = false;

        // Parse strings like "b3s23", "B3/S23", etc.
        for c in rule_str.to_ascii_uppercase().chars() {
            match c {
                'B' => {
                    parsing_b = true;
                    parsing_s = false;
                }
                'S' => {
                    parsing_s = true;
                    parsing_b = false;
                }
                '0'..='8' => {
                    let n = c.to_digit(10).unwrap() as usize;
                    if parsing_b {
                        b_rules[n] = true;
                    } else if parsing_s {
                        s_rules[n] = true;
                    }
                }
                _ => {} // Ignore slashes or unrecognized characters
            }
        }

        let mut zeropred = Vec::with_capacity(512);
        let mut onepred = Vec::with_capacity(512);
        let mut forward_lut = [CACell::DEAD; 512];

        // A 3x3 grid requires 9 bits. Range is 0..512
        for state in 0..512 as PackedCellsUnderlying {
            // Center cell is at x=1, y=1 -> bit index 4
            let center_alive = (state >> 4) & 1 != 0;

            // Total alive cells minus the center cell gives the neighbor count
            let neighbors = state.count_ones() - if center_alive { 1 } else { 0 };

            let becomes_alive = if center_alive {
                s_rules[neighbors as usize]
            } else {
                b_rules[neighbors as usize]
            };

            if becomes_alive {
                forward_lut[state as usize] = CACell::ALIVE;
                onepred.push(PackedCells(state));
            } else {
                zeropred.push(PackedCells(state));
            }
        }

        // Drop excess capacity
        zeropred.shrink_to_fit();
        onepred.shrink_to_fit();

        Self {
            zeropred,
            onepred,
            forward_lut,
        }
    }

    pub fn get_pred(&self, state: CACell) -> &[PackedCells] {
        match state {
            CACell::ALIVE => &self.onepred,
            CACell::DEAD => &self.zeropred,
        }
    }

    pub fn evolve(&self, neighborhood: PackedCells) -> CACell {
        debug_assert_eq!(neighborhood.0 & ((1 << 9) - 1), neighborhood.0);
        self.forward_lut[neighborhood.0 as usize]
    }

    pub fn get_score(&self, gen0: &PackedCells, _gen1: CACell) -> i32 {
        ((gen0.0 >> 4) & 1) as i32
    }
}
