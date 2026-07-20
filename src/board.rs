use crate::{
    basic_grids::*,
    ca::{CACell, RuleLut},
};
use rayon::prelude::*;

pub enum BoardWindowData {
    Leaf {
        valid_boards: Option<Vec<PackedCells>>,
    },
    SplitHorizontal {
        left: Box<BoardWindow>,
        right: Box<BoardWindow>,
        valid_combos: Option<Vec<(u64, u64)>>,
    },
    SplitVertical {
        top: Box<BoardWindow>,
        bottom: Box<BoardWindow>,
        // TODO:optimize mem by dynamic bit-size
        valid_combos: Option<Vec<(u64, u64)>>,
    },
}

pub struct BoardWindow {
    pub rect: Rect,
    pub inner: BoardWindowData,
    pub forced_dead: Directions,
    exposure_cache: [Option<Vec<PackedCells>>; 4],
    // Sorted `(exposure, original_index)` pairs (ascending by exposure). Storing the
    // exposure inline lets the merge loops in match/filter_exposures avoid calling
    // get_exposure per comparison (the hot path), and lets the sort extract each key
    // exactly once instead of O(n log n) times.
    exposure_sort_order_cache: [Option<Vec<(PackedCellsUnderlying, u64)>>; 4],
    score_cache: Option<Vec<i32>>,
}

impl BoardWindow {
    pub fn new(rect: Rect, forced_dead: Directions, ideal_h: f32, ideal_w: f32) -> Self {
        if rect.width() == 1 && rect.height() == 1 {
            return BoardWindow {
                rect,
                inner: BoardWindowData::Leaf { valid_boards: None },
                forced_dead,
                exposure_cache: [const { None }; 4],
                exposure_sort_order_cache: [const { None }; 4],
                score_cache: None,
            };
        }

        let inner = if (ideal_w > ideal_h && rect.width() != 1) || rect.height() == 1 {
            BoardWindowData::SplitHorizontal {
                left: Box::new(BoardWindow::new(
                    Rect::new(
                        rect.top,
                        rect.left,
                        rect.bottom,
                        rect.left + rect.width() / 2,
                    ),
                    forced_dead & !Directions::Right,
                    ideal_h,
                    ideal_w / 2.0,
                )),
                right: Box::new(BoardWindow::new(
                    Rect::new(
                        rect.top,
                        rect.left + rect.width() / 2,
                        rect.bottom,
                        rect.right,
                    ),
                    forced_dead & !Directions::Left,
                    ideal_h,
                    ideal_w / 2.0,
                )),
                valid_combos: None,
            }
        } else {
            BoardWindowData::SplitVertical {
                top: Box::new(BoardWindow::new(
                    Rect::new(
                        rect.top,
                        rect.left,
                        rect.top + rect.height() / 2,
                        rect.right,
                    ),
                    forced_dead & !Directions::Down,
                    ideal_h / 2.0,
                    ideal_w,
                )),
                bottom: Box::new(BoardWindow::new(
                    Rect::new(
                        rect.top + rect.height() / 2,
                        rect.left,
                        rect.bottom,
                        rect.right,
                    ),
                    forced_dead & !Directions::Up,
                    ideal_h / 2.0,
                    ideal_w,
                )),
                valid_combos: None,
            }
        };

        BoardWindow {
            rect,
            inner,
            forced_dead,
            exposure_cache: [const { None }; 4],
            exposure_sort_order_cache: [const { None }; 4],
            score_cache: None,
        }
    }

    pub fn get_exposure(&self, i: u64, direction: Direction) -> Option<PackedCells> {
        if let Some(cache) = &self.exposure_cache[direction as usize] {
            return Some(cache[i as usize]);
        }

        match &self.inner {
            BoardWindowData::Leaf {
                valid_boards: Some(valid_boards),
            } => {
                Some(valid_boards[i as usize].trim(self.get_padded_rect(), direction.opposite(), 1))
            }
            BoardWindowData::SplitHorizontal {
                left,
                right,
                valid_combos: Some(valid_combos),
            } => {
                let (i1, i2) = valid_combos[i as usize];
                match direction {
                    Direction::Up | Direction::Down => {
                        let left_erect = left.get_exposure_rect(direction);
                        let lexpo =
                            left.get_exposure(i1, direction)?
                                .trim(left_erect, Direction::Right, 2);
                        Some(lexpo.join(
                            right.get_exposure(i2, direction)?,
                            left_erect.trim(Direction::Right, 2),
                            right.get_exposure_rect(direction),
                            Direction::Right,
                        ))
                    }
                    Direction::Left => left.get_exposure(i1, direction),
                    Direction::Right => right.get_exposure(i2, direction),
                }
            }
            BoardWindowData::SplitVertical {
                top,
                bottom,
                valid_combos: Some(valid_combos),
            } => {
                let (i1, i2) = valid_combos[i as usize];
                match direction {
                    Direction::Up => top.get_exposure(i1, direction),
                    Direction::Down => bottom.get_exposure(i2, direction),
                    Direction::Left | Direction::Right => {
                        let top_erect = top.get_exposure_rect(direction);
                        let texpo =
                            top.get_exposure(i1, direction)?
                                .trim(top_erect, Direction::Down, 2);
                        Some(texpo.join(
                            bottom.get_exposure(i2, direction)?,
                            top_erect.trim(Direction::Down, 2),
                            bottom.get_exposure_rect(direction),
                            Direction::Down,
                        ))
                    }
                }
            }
            _ => None,
        }
    }

    pub fn get_exposure_rect(&self, direction: Direction) -> Rect {
        match direction {
            Direction::Up => Rect::new(
                self.rect.top - 1,
                self.rect.left - 1,
                self.rect.top + 1,
                self.rect.right + 1,
            ),
            Direction::Down => Rect::new(
                self.rect.bottom - 1,
                self.rect.left - 1,
                self.rect.bottom + 1,
                self.rect.right + 1,
            ),
            Direction::Left => Rect::new(
                self.rect.top - 1,
                self.rect.left - 1,
                self.rect.bottom + 1,
                self.rect.left + 1,
            ),
            Direction::Right => Rect::new(
                self.rect.top - 1,
                self.rect.right - 1,
                self.rect.bottom + 1,
                self.rect.right + 1,
            ),
        }
    }

    pub fn get_padded_rect(&self) -> Rect {
        Rect::new(
            self.rect.top - 1,
            self.rect.left - 1,
            self.rect.bottom + 1,
            self.rect.right + 1,
        )
    }

    pub fn get_num_valid_boards(&self) -> Option<u64> {
        match &self.inner {
            BoardWindowData::Leaf {
                valid_boards: Some(valid_boards),
            } => Some(valid_boards.len() as u64),
            BoardWindowData::SplitHorizontal {
                valid_combos: Some(valid_combos),
                ..
            } => Some(valid_combos.len() as u64),
            BoardWindowData::SplitVertical {
                valid_combos: Some(valid_combos),
                ..
            } => Some(valid_combos.len() as u64),
            _ => None,
        }
    }

    pub fn build_exposure_cache(&mut self, direction: Direction) {
        if self.exposure_cache[direction as usize].is_some() {
            return;
        }

        if let Some(num_boards) = self.get_num_valid_boards() {
            let mut cache = vec![PackedCells(0); num_boards as usize];

            for i in 0..num_boards {
                if let Some(exposure) = self.get_exposure(i, direction) {
                    cache[i as usize] = exposure;
                }
            }
            self.exposure_cache[direction as usize] = Some(cache);
        }
    }

    pub fn build_exposure_sort_order_cache(&mut self, direction: Direction) {
        if self.exposure_sort_order_cache[direction as usize].is_some() {
            return;
        }

        self.build_exposure_cache(direction);
        if let Some(expo) = &self.exposure_cache[direction as usize] {
            let mut cache: Vec<_> = expo
                .iter()
                .enumerate()
                .map(|(i, pc)| (pc.0, i as u64))
                .collect();
            cache.par_sort_unstable();
            self.exposure_sort_order_cache[direction as usize] = Some(cache)
        }
    }

    pub fn get_matches(
        order1: &[(PackedCellsUnderlying, u64)],
        order2: &[(PackedCellsUnderlying, u64)],
    ) -> Vec<((usize, usize), (usize, usize))> {
        let mut valid_combos = Vec::new();

        let mut i = 0;
        let mut j = 0;

        while i < order1.len() && j < order2.len() {
            let exp1 = order1[i].0;
            let exp2 = order2[j].0;

            if exp1 < exp2 {
                i += 1;
            } else if exp1 > exp2 {
                j += 1;
            } else {
                // Match found. Because multiple boards can yield the same boundary exposure,
                // we scan forward to find the full block of identical exposures on both sides.
                let mut i_end = i + 1;
                while i_end < order1.len() && order1[i_end].0 == exp1 {
                    i_end += 1;
                }

                let mut j_end = j + 1;
                while j_end < order2.len() && order2[j_end].0 == exp2 {
                    j_end += 1;
                }

                valid_combos.push(((i, i_end), (j, j_end)));

                i = i_end;
                j = j_end;
            }
        }
        valid_combos
    }

    pub fn cull_badscore_tillmatch(
        b1: &mut BoardWindow,
        direction: Direction,
        b2: &mut BoardWindow,
        patt_limit: usize,
        percull_frac: f64,
        verbosity: i32,
    ) -> Option<Vec<(u64, u64)>> {
        assert_eq!(
            b1.get_exposure_rect(direction),
            b2.get_exposure_rect(direction.opposite())
        );

        b1.build_exposure_cache(direction);
        b2.build_exposure_cache(direction.opposite());

        b1.build_exposure_sort_order_cache(direction);
        b2.build_exposure_sort_order_cache(direction.opposite());

        loop {
            let order1 = b1.exposure_sort_order_cache[direction as usize]
                .as_deref()
                .unwrap();
            let order2 = b2.exposure_sort_order_cache[direction.opposite() as usize]
                .as_deref()
                .unwrap();
            let valid_combos: Vec<((usize, usize), (usize, usize))> =
                Self::get_matches(order1, order2);
            let tot_combos = valid_combos
                .iter()
                .map(|&((a, b), (c, d))| (b - a) * (d - c))
                .sum::<usize>();
            if tot_combos <= patt_limit {
                if verbosity > 1 {
                    eprintln!("Combing at {:?} to {} boards.", b1.rect, tot_combos);
                }
                let mut valid_combos_unwrapped = Vec::with_capacity(tot_combos);
                for &(is1, is2) in &valid_combos {
                    for a in is1.0..is1.1 {
                        for b in is2.0..is2.1 {
                            valid_combos_unwrapped.push((order1[a].1, order2[b].1));
                        }
                    }
                }
                return Some(valid_combos_unwrapped);
            }
            if verbosity > 0 {
                eprintln!(
                    "Too many boards at {:?} ({}). culling some low scoring boards...",
                    b1.rect, tot_combos
                );
            }
            // time to eliminate
            b1.cull_worst_scores((order1.len() as f64 * (1. - percull_frac)) as usize);
            b2.cull_worst_scores((order2.len() as f64 * (1. - percull_frac)) as usize);
        }
    }

    pub fn match_exposures(
        b1: &mut BoardWindow,
        direction: Direction,
        b2: &mut BoardWindow,
        patt_limit: Option<usize>,
        verbosity: i32,
    ) -> Option<Vec<(u64, u64)>> {
        assert_eq!(
            b1.get_exposure_rect(direction),
            b2.get_exposure_rect(direction.opposite())
        );

        b1.build_exposure_cache(direction);
        b2.build_exposure_cache(direction.opposite());

        b1.build_exposure_sort_order_cache(direction);
        b2.build_exposure_sort_order_cache(direction.opposite());

        let order1 = b1.exposure_sort_order_cache[direction as usize]
            .as_deref()
            .unwrap();
        let order2 = b2.exposure_sort_order_cache[direction.opposite() as usize]
            .as_deref()
            .unwrap();

        let valid_combos: Vec<((usize, usize), (usize, usize))> = Self::get_matches(order1, order2);
        let tot_combos = valid_combos
            .iter()
            .map(|&((a, b), (c, d))| (b - a) * (d - c))
            .sum();

        let mut valid_combos_unwrapped = Vec::new();

        if let Some(lt) = patt_limit
            && lt < tot_combos
        {
            if verbosity > 0 {
                eprintln!(
                    "Had to cull at {:?} - {}/{} retained....",
                    b1.rect, lt, tot_combos
                );
            }
            let mut rng = rand::rng();
            let mut indices = rand::seq::index::sample(&mut rng, tot_combos, lt).into_vec();
            indices.par_sort_unstable();

            valid_combos_unwrapped.reserve(indices.len());
            let mut tot_combos_processed = 0;
            let mut tot_combos_finished = 0;
            let mut vc_i = 0;
            for i in indices {
                while tot_combos_processed <= i {
                    let (is1, is2) = valid_combos[vc_i];
                    tot_combos_finished = tot_combos_processed;
                    tot_combos_processed += (is1.1 - is1.0) * (is2.1 - is2.0);
                    vc_i += 1;
                }
                let (is1, is2) = valid_combos[vc_i - 1];
                let is2l = is2.1 - is2.0;
                valid_combos_unwrapped.push((
                    order1[is1.0 + (i - tot_combos_finished) / is2l].1,
                    order2[is2.0 + (i - tot_combos_finished) % is2l].1,
                ));
            }
        } else {
            if verbosity > 1 {
                eprintln!("Combing at {:?} to {} boards.", b1.rect, tot_combos);
            }
            valid_combos_unwrapped.reserve(tot_combos);
            for &(is1, is2) in &valid_combos {
                for a in is1.0..is1.1 {
                    for b in is2.0..is2.1 {
                        valid_combos_unwrapped.push((order1[a].1, order2[b].1));
                    }
                }
            }
        }

        Some(valid_combos_unwrapped)
    }

    /// Identifies matching boundaries, marks survivors, and purges all invalid
    /// board states and caches from both windows.
    pub fn filter_exposures(b1: &mut BoardWindow, direction: Direction, b2: &mut BoardWindow) {
        assert_eq!(
            b1.get_exposure_rect(direction),
            b2.get_exposure_rect(direction.opposite())
        );

        b1.build_exposure_cache(direction);
        b2.build_exposure_cache(direction.opposite());

        b1.build_exposure_sort_order_cache(direction);
        b2.build_exposure_sort_order_cache(direction.opposite());

        let order1 = b1.exposure_sort_order_cache[direction as usize]
            .as_ref()
            .unwrap();
        let order2 = b2.exposure_sort_order_cache[direction.opposite() as usize]
            .as_ref()
            .unwrap();

        let mut keep1 = vec![false; order1.len()];
        let mut keep2 = vec![false; order2.len()];

        for ((b1s, b1e), (b2s, b2e)) in Self::get_matches(order1, order2) {
            for &(_, i1) in &order1[b1s..b1e] {
                keep1[i1 as usize] = true;
            }
            for &(_, i2) in &order2[b2s..b2e] {
                keep2[i2 as usize] = true;
            }
        }

        b1.retain_valid_boards(&keep1);
        b2.retain_valid_boards(&keep2);
    }

    pub fn cull_worst_scores(&mut self, to_retain: usize) -> Option<()> {
        if to_retain as u64 >= self.get_num_valid_boards()? {
            return Some(());
        }
        self.build_score_cache();
        let mut pops = self
            .score_cache
            .as_deref()?
            .iter()
            .copied()
            .zip(0usize..)
            .collect::<Vec<_>>();
        pops.select_nth_unstable(to_retain);
        let mut keep = vec![false; pops.len()];
        for &(_, i) in &pops[..to_retain] {
            keep[i] = true;
        }
        self.retain_valid_boards(&keep);
        Some(())
    }

    pub fn retain_valid_boards(&mut self, keep: &[bool]) {
        // Filter the primary data structures
        match &mut self.inner {
            BoardWindowData::Leaf {
                valid_boards: Some(boards),
            } => {
                *boards = boards
                    .iter()
                    .zip(keep.iter())
                    .filter_map(|(&b, &k)| if k { Some(b) } else { None })
                    .collect();
            }
            BoardWindowData::SplitHorizontal {
                valid_combos: Some(combos),
                ..
            }
            | BoardWindowData::SplitVertical {
                valid_combos: Some(combos),
                ..
            } => {
                *combos = combos
                    .iter()
                    .zip(keep.iter())
                    .filter_map(|(&c, &k)| if k { Some(c) } else { None })
                    .collect();
            }
            _ => {}
        }

        for dir in 0..4 {
            // Compact the exposure bounds
            if let Some(cache) = &mut self.exposure_cache[dir] {
                *cache = cache
                    .iter()
                    .zip(keep.iter())
                    .filter_map(|(&c, &k)| if k { Some(c) } else { None })
                    .collect();
            }
        }

        // Compact the score cache
        if let Some(cache) = &mut self.score_cache {
            *cache = cache
                .iter()
                .zip(keep.iter())
                .filter_map(|(&c, &k)| if k { Some(c) } else { None })
                .collect();
        }

        // Build an O(N) mapping from old indices to new compacted indices
        let mut old_to_new = vec![0; keep.len()];
        let mut new_count = 0;
        for (i, &k) in keep.iter().enumerate() {
            if k {
                old_to_new[i] = new_count;
                new_count += 1;
            }
        }

        // Update all caches in O(N)
        for dir in 0..4 {
            // O(N) sort cache rebuild
            if let Some(sort_cache) = &mut self.exposure_sort_order_cache[dir] {
                // By iterating through the already-sorted array, we preserve the sort order.
                // We just drop the purged indices and remap the surviving ones.
                *sort_cache = sort_cache
                    .iter()
                    .filter(|&&(_, old_idx)| keep[old_idx as usize])
                    .map(|&(exp, old_idx)| (exp, old_to_new[old_idx as usize]))
                    .collect();
            }
        }
    }

    pub fn get_score(&self, i: u64) -> Option<i32> {
        if let Some(cache) = &self.score_cache {
            return Some(cache[i as usize]);
        }

        match &self.inner {
            BoardWindowData::SplitHorizontal {
                left,
                right,
                valid_combos: Some(valid_combos),
            } => {
                let (i1, i2) = valid_combos[i as usize];
                Some(left.get_score(i1)? + right.get_score(i2)?)
            }
            BoardWindowData::SplitVertical {
                top,
                bottom,
                valid_combos: Some(valid_combos),
            } => {
                let (i1, i2) = valid_combos[i as usize];
                Some(top.get_score(i1)? + bottom.get_score(i2)?)
            }
            _ => None,
        }
    }

    pub fn build_score_cache(&mut self) {
        if self.score_cache.is_some() {
            return;
        }

        if let Some(num_boards) = self.get_num_valid_boards() {
            let mut cache = vec![0; num_boards as usize];

            for i in 0..num_boards {
                if let Some(exposure) = self.get_score(i) {
                    cache[i as usize] = exposure;
                }
            }
            self.score_cache = Some(cache);
        }
    }

    // removes all high pop but equivalent
    pub fn remove_degenerate_lowscore(&mut self) -> Option<()> {
        const DIRECS: [Direction; 4] = [
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ];
        for d in DIRECS {
            self.build_exposure_cache(d);
        }
        self.build_score_cache();
        let pops = self.score_cache.as_deref()?;

        let nboard = self.get_num_valid_boards()? as usize;
        let mut tosort = vec![([PackedCells::ALL_DEAD; 4], 0); nboard];
        for i in 0..nboard {
            tosort[i] = (
                [
                    self.exposure_cache[0].as_deref()?[i],
                    self.exposure_cache[1].as_deref()?[i],
                    self.exposure_cache[2].as_deref()?[i],
                    self.exposure_cache[3].as_deref()?[i],
                ],
                i,
            )
        }
        tosort.par_sort_unstable();
        let mut tokeep = vec![false; nboard];

        let mut i = 0;
        while i < nboard {
            let mut j = i + 1;
            let mut bestidx = i;
            while j < nboard && tosort[j].0 == tosort[i].0 {
                if pops[bestidx] > pops[j] {
                    bestidx = j;
                }
                j += 1;
            }
            tokeep[bestidx] = true;
            i = j;
        }
        drop(tosort);
        self.retain_valid_boards(&tokeep);
        Some(())
    }

    pub fn extract_grid_at_depth<'a>(
        &'a mut self,
        depth: usize,
    ) -> Option<Grid<&'a mut BoardWindow>> {
        let mut flat: Vec<&'a mut BoardWindow> = Vec::new();
        self.collect_at_depth(depth, &mut flat)?;

        flat.sort_by(|a, b| {
            a.rect
                .top
                .cmp(&b.rect.top)
                .then(a.rect.left.cmp(&b.rect.left))
        });

        let leftmost = flat[0].rect.left;
        let width = flat
            .iter()
            .enumerate()
            .find_map(|(i, a)| {
                if i != 0 && a.rect.left == leftmost {
                    Some(i)
                } else {
                    None
                }
            })
            .unwrap_or(flat.len());

        Some(Grid::from_flat(width, flat))
    }

    /// Helper to recursively gather mutable references, safely splitting the borrow.
    fn collect_at_depth<'a>(
        &'a mut self,
        depth: usize,
        out: &mut Vec<&'a mut BoardWindow>,
    ) -> Option<()> {
        if depth == 0 {
            out.push(self);
            return Some(());
        }

        match &mut self.inner {
            BoardWindowData::Leaf { .. } => {
                // If we hit a 1x1 leaf before reaching depth k, it can't split further.
                None
            }
            BoardWindowData::SplitHorizontal { left, right, .. } => {
                // Rust allows multiple mutable borrows here because we are destructuring
                // disjoint fields (left and right) simultaneously.
                left.collect_at_depth(depth - 1, out)?;
                right.collect_at_depth(depth - 1, out)?;
                Some(())
            }
            BoardWindowData::SplitVertical { top, bottom, .. } => {
                top.collect_at_depth(depth - 1, out)?;
                bottom.collect_at_depth(depth - 1, out)?;
                Some(())
            }
        }
    }

    pub fn fill_leaves(
        &mut self,
        rule_lut: &RuleLut,
        gen0: &Grid<Option<CACell>>,
        gen1: &Grid<Option<CACell>>,
    ) -> Option<()> {
        let BoardWindow {
            rect: Rect { top, left, .. },
            inner,
            forced_dead,
            score_cache,
            ..
        } = self;
        match inner {
            BoardWindowData::Leaf { valid_boards } => {
                let mut candidates = Vec::new();
                let mut candidates_scores = Vec::new();
                let neighs_to_consider = if let &Some(gen1c) = gen1.get((*left, *top).into()) {
                    rule_lut.get_pred(gen1c)
                } else {
                    &(0..512)
                        .into_iter()
                        .map(|x| PackedCells(x))
                        .collect::<Vec<_>>()
                };

                let gen0c = *gen0.get((*left, *top).into());

                for &pred in neighs_to_consider {
                    if forced_dead.iter().all(|d| {
                        let d = d.to_single().unwrap();
                        let x33 = Rect::new(0, 0, 3, 3);
                        let x23 = x33.trim(d.opposite(), 1);
                        let x13 = x33.trim(d, 2);
                        pred.trim(x33, d.opposite(), 2) == PackedCells::ALL_DEAD
                            && rule_lut.evolve(pred.trim(x33, d.opposite(), 1).join(
                                PackedCells::ALL_DEAD,
                                x23,
                                x13,
                                d.opposite(),
                            )) == CACell::DEAD
                    }) && gen0c.map_or(true, |x| {
                        <CACell as Into<u8>>::into(x) == ((pred.0 >> 4) & 1) as u8
                    }) {
                        candidates.push(pred);
                        candidates_scores.push(rule_lut.get_score(&pred, rule_lut.evolve(pred)));
                    }
                }
                *valid_boards = Some(candidates);
                *score_cache = Some(candidates_scores);
            }
            BoardWindowData::SplitHorizontal { left, right, .. } => {
                left.fill_leaves(rule_lut, gen0, gen1)?;
                right.fill_leaves(rule_lut, gen0, gen1)?;
            }
            BoardWindowData::SplitVertical { top, bottom, .. } => {
                top.fill_leaves(rule_lut, gen0, gen1)?;
                bottom.fill_leaves(rule_lut, gen0, gen1)?;
            }
        }
        Some(())
    }

    /// Extracts the full configuration of the board for a given index `i`.
    /// The resulting Grid represents the padded area of this BoardWindow.
    pub fn extract_board(&self, i: u64) -> Option<Grid<CACell>> {
        let padded_rect = self.get_padded_rect();

        // Initialize the grid with the dimensions of the padded rect.
        let mut grid = Grid::new(padded_rect.height(), padded_rect.width(), CACell::DEAD);

        // Override the default (0, 0) top-left to use absolute coordinates,
        // assuming `Grid::get_mut` respects `grid.size`.
        grid.size = padded_rect;

        self.write_to_grid(i, &mut grid, true)?;
        Some(grid)
    }

    /// Recursively walks the tree and populates the grid with leaf cell states.
    pub fn write_to_grid(&self, i: u64, grid: &mut Grid<CACell>, write_border: bool) -> Option<()> {
        match &self.inner {
            BoardWindowData::Leaf {
                valid_boards: Some(valid_boards),
            } => {
                let board = valid_boards[i as usize];
                let pr = self.get_padded_rect();
                if write_border {
                    // A leaf cell is 1x1, meaning its padded rect is exactly 3x3.
                    for y in 0..3 {
                        for x in 0..3 {
                            let is_alive = (board.0 >> (y * 3 + x)) & 1 == 1;
                            let global_y = pr.top + y;
                            let global_x = pr.left + x;
                            *grid.get_mut((global_x, global_y).into()) = if is_alive {
                                CACell::ALIVE
                            } else {
                                CACell::DEAD
                            };
                        }
                    }
                } else {
                    let is_alive = (board.0 >> 4) & 1 == 1;
                    let global_y = pr.top + 1;
                    let global_x = pr.left + 1;
                    *grid.get_mut((global_x, global_y).into()) = if is_alive {
                        CACell::ALIVE
                    } else {
                        CACell::DEAD
                    };
                }
                Some(())
            }
            BoardWindowData::SplitHorizontal {
                left,
                right,
                valid_combos: Some(valid_combos),
            } => {
                let (i1, i2) = valid_combos[i as usize];
                left.write_to_grid(i1, grid, write_border)?;
                right.write_to_grid(i2, grid, write_border)?;
                Some(())
            }
            BoardWindowData::SplitVertical {
                top,
                bottom,
                valid_combos: Some(valid_combos),
            } => {
                let (i1, i2) = valid_combos[i as usize];
                top.write_to_grid(i1, grid, write_border)?;
                bottom.write_to_grid(i2, grid, write_border)?;
                Some(())
            }
            _ => None,
        }
    }

    /// Smallest depth at which a leaf occurs. For depths `d <= min_leaf_depth`
    /// every root-to-node path is still internal, so `collect_at_depth(d)` /
    /// `fill_combinations(.., d, ..)` are guaranteed to reach a clean, complete
    /// grid of `2^d` nodes (important for non-power-of-two rectangles like 5x5,
    /// where deeper depths hit leaves in the short subtrees first).
    pub fn min_leaf_depth(&self) -> usize {
        match &self.inner {
            BoardWindowData::Leaf { .. } => 0,
            BoardWindowData::SplitHorizontal { left, right, .. } => {
                1 + left.min_leaf_depth().min(right.min_leaf_depth())
            }
            BoardWindowData::SplitVertical { top, bottom, .. } => {
                1 + top.min_leaf_depth().min(bottom.min_leaf_depth())
            }
        }
    }

    pub fn free_caches(&mut self, active_depth: i32) {
        self.exposure_sort_order_cache = [const { None }; 4];
        if active_depth != 0 {
            self.exposure_cache = [const { None }; 4];
        }
        match &mut self.inner {
            BoardWindowData::SplitHorizontal { left, right, .. } => {
                left.free_caches(active_depth - 1);
                right.free_caches(active_depth - 1);
            }
            BoardWindowData::SplitVertical { top, bottom, .. } => {
                top.free_caches(active_depth - 1);
                bottom.free_caches(active_depth - 1);
            }
            _ => (),
        }
    }
}
