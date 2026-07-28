use crate::basic_grids::*;
use crate::board::*;
use rayon::prelude::*;

fn get_npatt_log<'a>(boards: impl Iterator<Item = &'a BoardWindow>) -> Option<f64> {
    let mut npatt_log = 0.;
    for b in boards {
        npatt_log += (b.get_num_valid_boards()? as f64).log10();
    }
    Some(npatt_log)
}

pub fn extract_and_cull_lowcost(
    board: &mut BoardWindow,
    depth: usize,
    verbosity: i32,
) -> Option<()> {
    let mut g = board.extract_grid_at_depth(depth)?;
    g.data
        .par_iter_mut()
        .try_for_each(|b| b.remove_degenerate_lowcost())?;
    if verbosity > 0 {
        eprintln!(
            "New log_10(patterns) after culling highpop duplicates -- {}",
            get_npatt_log(g.data.iter().map(|a| &**a))?
        );
    }
    Some(())
}

pub fn extract_and_cull(
    board: &mut BoardWindow,
    depth: usize,
    stop_eps: f64,
    verbosity: i32,
) -> Option<()> {
    let mut g = board.extract_grid_at_depth(depth)?;
    let mut direc = Direction::Right;

    let mut prev_npatt_log = get_npatt_log(g.data.iter().map(|a| &**a))?;
    if verbosity > 0 {
        eprintln!("log_10(patterns) before culling -- {prev_npatt_log}");
    }

    loop {
        for _ in 0..2 {
            let mut rows = g.into_rows_mut();
            // Perform a cull (TODO:parallelize)

            rows.par_iter_mut().for_each(|row| {
                for i in 0..row.len() - 1 {
                    if let [w1, w2] = &mut row[i..=i + 1] {
                        BoardWindow::filter_exposures(w1, direc, w2);
                    } else {
                        unreachable!();
                    }
                }
            });

            g.rotate_90_cw_inplace();
            direc = direc.rotate_90_cw().opposite();
        }

        let npatt_log = get_npatt_log(g.data.iter().map(|a| &**a))?;
        if verbosity > 0 {
            eprintln!("log_10(patterns) after a round of culling -- {npatt_log}");
        }
        if prev_npatt_log - stop_eps <= npatt_log {
            break;
        }
        prev_npatt_log = npatt_log;
    }

    Some(())
}

pub enum CombinerCfg {
    Exhaustive,
    KeepHighCost { patt_limit: usize, cullfrac: f64 },
    KeepRandom { patt_limit: usize },
}

pub fn fill_combinations(board: &mut BoardWindow, depth: usize, cfg: &CombinerCfg, verbosity: i32) {
    if depth == 0 {
        // Target depth reached; compute all combinations down to the leaves for this subtree
        fill_all_combinations(board, cfg, verbosity)
    } else {
        match &mut board.inner {
            BoardWindowData::Leaf { .. } => {
                panic!("depth error")
            }
            BoardWindowData::SplitHorizontal { left, right, .. } => {
                rayon::join(
                    || fill_combinations(left, depth - 1, cfg, verbosity),
                    || fill_combinations(right, depth - 1, cfg, verbosity),
                );
            }
            BoardWindowData::SplitVertical { top, bottom, .. } => {
                rayon::join(
                    || fill_combinations(top, depth - 1, cfg, verbosity),
                    || fill_combinations(bottom, depth - 1, cfg, verbosity),
                );
            }
        }
    }
}

pub fn fill_all_combinations(board: &mut BoardWindow, cfg: &CombinerCfg, verbosity: i32) {
    match &mut board.inner {
        BoardWindowData::Leaf { valid_boards } => {
            // Leaves are assumed to already have valid_boards populated by fill_leaves
            valid_boards.as_deref().expect("Board not filled yet");
        }
        BoardWindowData::SplitHorizontal {
            left,
            right,
            valid_combos,
        } if *valid_combos == None => {
            // Populate children first
            rayon::join(
                || fill_all_combinations(left, cfg, verbosity),
                || fill_all_combinations(right, cfg, verbosity),
            );

            // Match exposures along the shared vertical boundary
            let combos = match *cfg {
                CombinerCfg::Exhaustive => {
                    BoardWindow::match_exposures(left, Direction::Right, right, None, verbosity)
                        .unwrap()
                }
                CombinerCfg::KeepHighCost {
                    patt_limit,
                    cullfrac,
                } => BoardWindow::cull_badcost_tillmatch(
                    left,
                    Direction::Right,
                    right,
                    patt_limit,
                    cullfrac,
                    verbosity,
                )
                .unwrap(),
                CombinerCfg::KeepRandom { patt_limit } => BoardWindow::match_exposures(
                    left,
                    Direction::Right,
                    right,
                    Some(patt_limit),
                    verbosity,
                )
                .unwrap(),
            };

            *valid_combos = Some(combos);
        }
        BoardWindowData::SplitVertical {
            top,
            bottom,
            valid_combos,
        } if *valid_combos == None => {
            // Populate children first
            rayon::join(
                || fill_all_combinations(top, cfg, verbosity),
                || fill_all_combinations(bottom, cfg, verbosity),
            );

            // Match exposures along the shared horizontal boundary
            let combos = match *cfg {
                CombinerCfg::Exhaustive => {
                    BoardWindow::match_exposures(top, Direction::Down, bottom, None, verbosity)
                        .unwrap()
                }
                CombinerCfg::KeepHighCost {
                    patt_limit,
                    cullfrac,
                } => BoardWindow::cull_badcost_tillmatch(
                    top,
                    Direction::Down,
                    bottom,
                    patt_limit,
                    cullfrac,
                    verbosity,
                )
                .unwrap(),
                CombinerCfg::KeepRandom { patt_limit } => BoardWindow::match_exposures(
                    top,
                    Direction::Down,
                    bottom,
                    Some(patt_limit),
                    verbosity,
                )
                .unwrap(),
            };

            *valid_combos = Some(combos);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Exhaustiveness testing
// ---------------------------------------------------------------------------
//
// The quadtree search is only useful if its culling + boundary-joining never
// drop a genuinely-valid predecessor. Here we build an independent brute-force
// ground truth by enumerating *every* 5x5 board (2^25) with rayon, keeping the
// ones whose forward evolution stays inside a 5x5 (i.e. the 7x7 successor's
// outer ring is all dead), and grouping them by the 5x5 target they produce.
//
// Then, for chosen targets (5x5 patterns, and 4x4 patterns embedded in a 5x5),
// we run the real search with *no* `patt_limit` (so nothing is ever truncated,
// per the `match_exposures` gotcha) and assert the set of predecessors it finds
// is *exactly* the brute-force set — no missing (culling/joining must be
// exhaustive) and no extra (they must be sound).
//
// NOTE: the 2^25 brute-force pass is heavy; run these with `--release`:
//   cargo test --release exhaustive
//   cargo test --release exhaustive_5x5_all -- --ignored --nocapture

/// 5x5 boards are bit-packed into a `u32`, `bit(y*5 + x)`.
#[cfg(test)]
pub mod exhaustive {
    use super::*;
    use crate::ca::{CACell, RuleLut};
    use std::collections::{HashMap, HashSet};

    const N: i32 = 5;
    const NBOARDS: u32 = 1 << (N * N); // 2^25

    #[inline]
    fn cell(p: u32, x: i32, y: i32) -> PackedCellsUnderlying {
        if x >= 0 && x < N && y >= 0 && y < N {
            ((p >> (y * N + x)) & 1) as PackedCellsUnderlying
        } else {
            0 // outside the 5x5 == part of the dead infinite plane
        }
    }

    /// Build the 9-bit 3x3 neighbourhood centred at `(cx, cy)` in the layout
    /// `RuleLut::evolve` expects (`bit((dy+1)*3 + (dx+1))`, centre = bit 4).
    #[inline]
    fn neighborhood(p: u32, cx: i32, cy: i32) -> PackedCells {
        let mut n = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let idx = (dy + 1) * 3 + (dx + 1);
                n |= cell(p, cx + dx, cy + dy) << idx;
            }
        }
        PackedCells(n)
    }

    /// Evolve a 5x5 board `p` embedded in an infinite dead plane. Returns the
    /// 5x5 successor `Some(target)` iff the successor is fully contained in the
    /// same 5x5 window (every cell of the surrounding 7x7 ring is dead);
    /// otherwise `None` (the pattern spilled out — not a 5x5 -> 5x5 step).
    pub fn evolve_5x5(rule: &RuleLut, p: u32) -> Option<u32> {
        // Reject early if anything is born in the 7x7 border ring.
        for y in -1..=N {
            for x in -1..=N {
                let on_ring = x == -1 || x == N || y == -1 || y == N;
                if on_ring && rule.evolve(neighborhood(p, x, y)).unwrap() == CACell::ALIVE {
                    return None;
                }
            }
        }
        let mut t = 0u32;
        for y in 0..N {
            for x in 0..N {
                if rule.evolve(neighborhood(p, x, y)).unwrap() == CACell::ALIVE {
                    t |= 1 << (y * N + x);
                }
            }
        }
        Some(t)
    }

    /// One full 2^25 brute-force sweep: `target -> {all 5x5 predecessors}`.
    /// This is the ground truth every search result is checked against.
    pub fn build_ground_truth(rule: &RuleLut) -> HashMap<u32, Vec<u32>> {
        (0..NBOARDS)
            .into_par_iter()
            .filter_map(|p| evolve_5x5(rule, p).map(|t| (t, p)))
            .fold(HashMap::<u32, Vec<u32>>::new, |mut acc, (t, p)| {
                acc.entry(t).or_default().push(p);
                acc
            })
            .reduce(HashMap::<u32, Vec<u32>>::new, |mut a, b| {
                for (t, mut ps) in b {
                    a.entry(t).or_default().append(&mut ps);
                }
                a
            })
    }

    /// Turn a packed 5x5 `target` into the `Grid<CACell>` the search consumes.
    fn target_grid(target: u32) -> Grid<CACell> {
        let mut g = Grid::new(N, N, CACell::DEAD);
        for y in 0..N {
            for x in 0..N {
                if (target >> (y * N + x)) & 1 == 1 {
                    *g.get_mut((x, y).into()) = CACell::ALIVE;
                }
            }
        }
        g
    }

    /// Read the central 5x5 (the actual predecessor cells) out of an extracted
    /// solution grid and pack it back into a `u32`.
    fn grid_to_u32(g: &Grid<CACell>) -> u32 {
        let mut p = 0u32;
        for y in 0..N {
            for x in 0..N {
                if *g.get((x, y).into()) == CACell::ALIVE {
                    p |= 1 << (y * N + x);
                }
            }
        }
        p
    }

    /// Run the real quadtree search on `target`, with unbounded `patt_limit`
    /// (no truncation) and `cull` optionally enabled, and return the set of
    /// distinct 5x5 predecessors it reports.
    pub fn search_predecessors(rule: &RuleLut, target: u32, cull: bool) -> HashSet<u32> {
        let grid = target_grid(target);
        let mut bw = BoardWindow::new(
            grid.size,
            Directions::all(),
            grid.size.height() as f32,
            grid.size.width() as f32,
        );
        bw.fill_leaves(
            &rule,
            &Grid::from_rect(bw.rect, None),
            &grid.map(|&x| Some(x)),
        );

        if cull {
            // Cull from the deepest *clean* depth (all paths still internal)
            // up to the root. Unbounded limits => joins never truncate.
            for d in (1..=bw.min_leaf_depth()).rev() {
                fill_combinations(&mut bw, d, &CombinerCfg::Exhaustive, 0);
                extract_and_cull(&mut bw, d, 0.0, 0).unwrap();
                bw.free_caches(d as i32);
            }
        }

        // Final exact combine down to the leaves.
        fill_combinations(&mut bw, 0, &CombinerCfg::Exhaustive, 0);

        let n = bw.get_num_valid_boards().unwrap();
        let mut out = HashSet::with_capacity(n as usize);
        for i in 0..n {
            out.insert(grid_to_u32(&bw.extract_board(i).unwrap()));
        }
        out
    }

    /// Compare the search (with and without culling) against `expected` and
    /// panic with a readable diff on any discrepancy.
    fn assert_exhaustive(rule: &RuleLut, target: u32, expected: &HashSet<u32>) {
        for &cull in &[false, true] {
            let found = search_predecessors(rule, target, cull);
            if &found != expected {
                let missing: Vec<u32> = expected.difference(&found).copied().collect();
                let extra: Vec<u32> = found.difference(expected).copied().collect();
                panic!(
                    "target {:#027b} (cull={}): expected {} preds, got {} \
                     ({} MISSING, {} EXTRA)\n  missing: {:?}\n  extra: {:?}",
                    target,
                    cull,
                    expected.len(),
                    found.len(),
                    missing.len(),
                    extra.len(),
                    missing.iter().take(8).collect::<Vec<_>>(),
                    extra.iter().take(8).collect::<Vec<_>>(),
                );
            }
        }
    }

    /// bbox of the live cells of a packed 5x5, as (w, h). Empty board -> (0, 0).
    fn bbox(t: u32) -> (i32, i32) {
        let (mut minx, mut miny, mut maxx, mut maxy) = (N, N, -1, -1);
        for y in 0..N {
            for x in 0..N {
                if (t >> (y * N + x)) & 1 == 1 {
                    minx = minx.min(x);
                    miny = miny.min(y);
                    maxx = maxx.max(x);
                    maxy = maxy.max(y);
                }
            }
        }
        if maxx < 0 {
            (0, 0)
        } else {
            (maxx - minx + 1, maxy - miny + 1)
        }
    }

    /// Default sample test: build the full ground truth once, then verify a
    /// spread of targets — some genuinely 5x5 (bbox touches all sides), some
    /// with a live bbox that fits in 4x4 (i.e. a 4x4 pattern embedded in 5x5).
    /// Overridable count via `EXHAUSTIVE_SAMPLE` (default 40).
    #[test]
    fn exhaustive_5x5_sample() {
        let rule = RuleLut::cost_as_population_from_rule("B3S23");
        let truth = build_ground_truth(&rule);
        assert!(!truth.is_empty(), "ground truth should be non-empty");

        let sample: usize = std::env::var("EXHAUSTIVE_SAMPLE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40);

        // Keys are already effectively randomly ordered by HashMap; split them
        // into "big" (spans 5 in some axis) and "small" (fits in 4x4) buckets
        // so we always cover both the 5x5 and 4x4-embedded cases.
        let mut big = Vec::new();
        let mut small = Vec::new();
        for &t in truth.keys() {
            let (w, h) = bbox(t);
            if w >= N || h >= N {
                big.push(t);
            } else {
                small.push(t);
            }
        }
        big.sort_unstable();
        small.sort_unstable();

        // Stride across each sorted bucket so we cover the whole target space
        // (sparse *and* dense patterns), not just the lowest-valued targets.
        let stride_take = |v: &[u32], k: usize| -> Vec<u32> {
            if v.is_empty() || k == 0 {
                return Vec::new();
            }
            let k = k.min(v.len());
            let step = (v.len() / k).max(1);
            (0..k).map(|i| v[i * step]).collect()
        };

        let half = sample / 2;
        let mut picks = stride_take(&big, half);
        picks.extend(stride_take(&small, sample - half));
        assert!(!picks.is_empty());

        eprintln!(
            "ground truth: {} distinct targets, {} big / {} small; testing {} targets",
            truth.len(),
            big.len(),
            small.len(),
            picks.len(),
        );

        for t in picks {
            let expected: HashSet<u32> = truth[&t].iter().copied().collect();
            assert_exhaustive(&rule, t, &expected);
        }
    }

    /// Exhaustively verify EVERY achievable 5x5 target. Very heavy — ignored by
    /// default; run explicitly with `--release`:
    ///   cargo test --release exhaustive_5x5_all -- --ignored --nocapture
    #[test]
    #[ignore]
    fn exhaustive_5x5_all() {
        let rule = RuleLut::cost_as_population_from_rule("B3S23");
        let truth = build_ground_truth(&rule);
        eprintln!("verifying all {} achievable targets", truth.len());

        let mut targets: Vec<u32> = truth.keys().copied().collect();
        targets.sort_unstable();
        for (i, t) in targets.iter().enumerate() {
            let expected: HashSet<u32> = truth[t].iter().copied().collect();
            assert_exhaustive(&rule, *t, &expected);
            if i % 500 == 0 {
                eprintln!("  {}/{} targets ok", i, targets.len());
            }
        }
    }
}
