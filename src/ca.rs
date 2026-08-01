use crate::basic_grids::{Coord, Grid, PackedCells, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CACell {
    DEAD,
    ALIVE,
}

impl CACell {
    pub fn into_u8(self) -> u8 {
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
    transition_allowed: [bool; 1024],
    transition_cost: [i32; 1024], //ruletable format reinterpreted as an int
    pub weightage_grid: Option<Grid<i32>>,
}

impl RuleLut {
    fn get_idx(gen0: PackedCells, gen1: CACell) -> usize {
        //need to rearrage from packedcells to golly format (NW,N,NE,W,C,E,SW,S,SE to C,N,NE,E,SE,S,SW,W,NW)
        let gen0 = gen0.0 as usize;
        let gen0 = (gen0 & 0x046)              // Keep N, NE, SW in place
         | ((gen0 & 0x0A0) >> 2)       // Move E, S down by 2 (bits 5, 7 -> 3, 5)
         | ((gen0 & 0x110) >> 4)       // Move C, SE down by 4 (bits 4, 8 -> 0, 4)
         | ((gen0 & 0x008) << 4)       // Move W up by 4 (bit 3 -> 7)
         | ((gen0 & 0x001) << 8); // Move NW up by 8 (bit 0 -> 8)
        (gen0 << 1) | gen1.into_u8() as usize
    }

    pub fn generate_count_pop_cfg(rule_str: &str) -> String {
        let mut b_req = [false; 9];
        let mut s_req = [false; 9];

        let rule_upper = rule_str.to_uppercase();

        if !rule_upper.starts_with('B') {
            panic!(
                "Strict format violation: Rule must start with 'B' (e.g., 'B3S23'). Found: {}",
                rule_str
            );
        }

        let s_idx = rule_upper.find('S').unwrap_or_else(|| {
            panic!("Strict format violation: Rule must contain exactly one 'S' separating birth and survival conditions. Found: {}", rule_str);
        });

        let (b_str, s_str) = rule_upper.split_at(s_idx);
        let b_str = &b_str[1..]; // Drop the 'B'
        let s_str = &s_str[1..]; // Drop the 'S'

        if s_str.contains('S') || s_str.contains('B') {
            panic!(
                "Strict format violation: Multiple 'B' or 'S' characters found. Found: {}",
                rule_str
            );
        }

        let parse_part = |part_str: &str, req: &mut [bool; 9], part_name: &str| {
            for c in part_str.chars() {
                let digit = c.to_digit(10).unwrap_or_else(|| {
                    panic!("Strict format violation: Invalid character '{}' in {} section. Only digits 0-8 are allowed.", c, part_name);
                });

                if digit > 8 {
                    panic!(
                        "Strict format violation: Digit '{}' out of bounds in {} section. Moore neighborhood max is 8.",
                        digit, part_name
                    );
                }

                if req[digit as usize] {
                    panic!(
                        "Strict format violation: Duplicate digit '{}' in {} section.",
                        digit, part_name
                    );
                }

                req[digit as usize] = true;
            }
        };

        parse_part(b_str, &mut b_req, "Birth");
        parse_part(s_str, &mut s_req, "Survival");

        let mut cfg = String::with_capacity(512 * 24);

        for gen0_val in 0..512 {
            // Extract bits based on our packed structure (NW=8 .. SE=0)
            let se = (gen0_val >> 0) & 1;
            let s = (gen0_val >> 1) & 1;
            let sw = (gen0_val >> 2) & 1;
            let e = (gen0_val >> 3) & 1;
            let c = (gen0_val >> 4) & 1;
            let w = (gen0_val >> 5) & 1;
            let ne = (gen0_val >> 6) & 1;
            let n = (gen0_val >> 7) & 1;
            let nw = (gen0_val >> 8) & 1;

            let neighbors = n + ne + e + se + s + sw + w + nw;

            let c_prime = if c == 1 {
                if s_req[neighbors as usize] { 1 } else { 0 }
            } else {
                if b_req[neighbors as usize] { 1 } else { 0 }
            };

            let score = c as i32;

            // Output in Golly order: C, N, NE, E, SE, S, SW, W, NW, C' = score
            cfg.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}={}\n",
                c, n, ne, e, se, s, sw, w, nw, c_prime, score
            ));
        }

        cfg
    }

    /// Parses the custom Golly-like CFG format where each line is:
    /// C,N,NE,E,SE,S,SW,W,NW,C'=score
    pub fn from_cfg(cfg: &str) -> Self {
        let mut transition_allowed = [false; 1024];
        let mut transition_cost = [0; 1024];

        for (line_no, line) in cfg.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('=').collect();
            if parts.len() != 2 {
                panic!(
                    "CFG parse error on line {}: missing '=' separator for score.",
                    line_no + 1
                );
            }

            let score: i32 = parts[1].trim().parse().unwrap_or_else(|_| {
                panic!(
                    "CFG parse error on line {}: invalid score integer.",
                    line_no + 1
                );
            });

            let states: Vec<u8> = parts[0]
                .split(',')
                .map(|s| {
                    s.trim().parse().unwrap_or_else(|_| {
                        panic!(
                            "CFG parse error on line {}: invalid state. Must be 0 or 1.",
                            line_no + 1
                        );
                    })
                })
                .collect();

            if states.len() != 10 {
                panic!(
                    "CFG parse error on line {}: expected exactly 10 states (9 neighbors + 1 output). Found {}.",
                    line_no + 1,
                    states.len()
                );
            }

            // Golly ordering back to individual bits
            let c = states[0] as usize;
            let n = states[1] as usize;
            let ne = states[2] as usize;
            let e = states[3] as usize;
            let se = states[4] as usize;
            let s = states[5] as usize;
            let sw = states[6] as usize;
            let w = states[7] as usize;
            let nw = states[8] as usize;
            let c_prime = states[9];

            // Reconstruct gen0 packed int mapping (NW=8 .. SE=0)
            let gen0 = (nw << 8)
                | (n << 7)
                | (ne << 6)
                | (w << 5)
                | (c << 4)
                | (e << 3)
                | (sw << 2)
                | (s << 1)
                | se;
            let gen1 = if c_prime == 1 {
                CACell::ALIVE
            } else {
                CACell::DEAD
            };

            let idx = Self::get_idx(PackedCells(gen0 as _), gen1);

            // Register this transition as allowed and set its cost
            transition_allowed[idx] = true;
            transition_cost[idx] = score;
        }

        Self {
            transition_allowed,
            transition_cost,
            weightage_grid: None,
        }
    }

    /// Pipes the generator directly into the parser.
    #[allow(unused)]
    pub fn cost_as_population_from_rule(rule_str: &str) -> Self {
        let cfg_string = Self::generate_count_pop_cfg(rule_str);
        Self::from_cfg(&cfg_string)
    }

    pub fn get_pieces(&self, gen1: CACell, p: Coord) -> impl Iterator<Item = (PackedCells, i32)> {
        (0..1 << 9).into_iter().filter_map(move |gen0| {
            let i = Self::get_idx(PackedCells(gen0), gen1);
            if self.transition_allowed[i] {
                Some((
                    PackedCells(gen0),
                    self.transition_cost[i] * self.weightage_grid.as_ref().map_or(1, |g| *g.get(p)),
                ))
            } else {
                None
            }
        })
    }

    pub fn filter_pieces(
        &self,
        gen0: Option<CACell>,
        gen1: Option<CACell>,
        p: Coord,
    ) -> impl Iterator<Item = (PackedCells, i32)> {
        let mut outp = vec![];
        for g1 in [CACell::DEAD, CACell::ALIVE] {
            if gen1.is_none_or(|gen1| gen1 == g1) {
                outp.append(&mut self.get_pieces(g1, p).collect());
            }
        }
        outp.into_iter().filter(move |(g0, _)| {
            gen0.is_none_or(|gen0| ((g0.0 >> 4) & 1) as u8 == gen0.into_u8())
        })
    }

    pub fn evolve(&self, gen0: PackedCells) -> Option<CACell> {
        let ia = Self::get_idx(gen0, CACell::ALIVE);
        let id = Self::get_idx(gen0, CACell::DEAD);
        if self.transition_allowed[ia] != self.transition_allowed[id] {
            Some(if self.transition_allowed[ia] {
                CACell::ALIVE
            } else {
                CACell::DEAD
            })
        } else {
            None
        }
    }
}
