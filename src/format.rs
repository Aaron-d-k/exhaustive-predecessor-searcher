use std::fmt::Write as _;

use crate::{
    basic_grids::{Coord, Direction, Grid},
    ca::CACell,
};

// ============================================================================
// Grid<CACell> Life parsing and manipulation
// ============================================================================

impl<T> Grid<T> {
    /// Render as a `*` / `.` grid.
    pub fn to_plaintext_generic(&self, f: impl Fn(&T) -> char) -> String {
        let w = self.size.width();
        let h = self.size.height();
        let mut out = String::with_capacity(((w + 1) * h) as usize);

        for y in self.size.top..self.size.bottom {
            for x in self.size.left..self.size.right {
                out.push(f(self.get(Coord { x, y })));
            }
            out.push('\n');
        }
        out
    }
}

impl Grid<CACell> {
    /// Render as a `*` / `.` grid.
    pub fn to_plaintext(&self) -> String {
        let w = self.size.width();
        let h = self.size.height();
        let mut out = String::with_capacity(((w + 1) * h) as usize);

        for y in self.size.top..self.size.bottom {
            for x in self.size.left..self.size.right {
                let cell = self.get(Coord { x, y });
                out.push(match cell {
                    CACell::ALIVE => '*',
                    CACell::DEAD => '.',
                });
            }
            out.push('\n');
        }
        out
    }

    /// Render as pretty RLE: `o`/`b` per cell, rows separated by `$`, ended `!`.
    pub fn to_pretty_rle(&self, rule_str: &str) -> String {
        let w = self.size.width();
        let h = self.size.height();
        let mut out = String::new();

        let _ = writeln!(out, "x = {}, y = {}, rule = {}", w, h, rule_str);
        for y in self.size.top..self.size.bottom {
            for x in self.size.left..self.size.right {
                let cell = self.get(Coord { x, y });
                out.push(match cell {
                    CACell::ALIVE => 'o',
                    CACell::DEAD => 'b',
                });
            }
            out.push(if y == h - 1 { '!' } else { '$' });
            out.push('\n');
        }
        out
    }

    /// Return a copy padded with `border` dead cells on every side.
    pub fn with_border(&self, border: usize) -> Self {
        let b = border as i32;
        let old_size = self.size;

        let w = old_size.width() + 2 * b;
        let h = old_size.height() + 2 * b;

        let mut new_grid = Grid::new(h, w, CACell::DEAD);
        new_grid.set_origin((old_size.left - b, old_size.top - b).into());
        for y in old_size.top..old_size.bottom {
            for x in old_size.left..old_size.right {
                *new_grid.get_mut(Coord { x, y }) = *self.get(Coord { x, y });
            }
        }
        new_grid
    }

    /// Return a copy padded with `border` dead cells on a specific side.
    pub fn with_border_direction(&self, border: usize, direction: Direction) -> Self {
        let b = border as i32;
        let old_size = self.size;

        let mut w = old_size.width();
        let mut h = old_size.height();

        let mut dx = 0;
        let mut dy = 0;

        match direction {
            Direction::Up => {
                h += b;
                dy -= b;
            }
            Direction::Down => {
                h += b;
            }
            Direction::Left => {
                w += b;
                dx -= b
            }
            Direction::Right => {
                w += b;
            }
        }

        let mut new_grid = Grid::new(h, w, CACell::DEAD);
        new_grid.set_origin((old_size.left + dx, old_size.top + dy).into());
        for y in old_size.top..old_size.bottom {
            for x in old_size.left..old_size.right {
                *new_grid.get_mut(Coord { x, y }) = *self.get(Coord { x, y });
            }
        }
        new_grid
    }

    /// Return a copy with `border` cells removed from every side.
    pub fn without_border(&self, border: usize) -> Self {
        let b = border as i32;
        let old_size = self.size;

        let w = 0.max(old_size.width() - 2 * b);
        let h = 0.max(old_size.height() - 2 * b);

        let mut new_grid = Grid::new(h, w, CACell::DEAD);
        new_grid.set_origin((old_size.left + b, old_size.top + b).into());
        for y in new_grid.size.top..new_grid.size.bottom {
            for x in new_grid.size.left..new_grid.size.right {
                *new_grid.get_mut(Coord { x, y }) = *self.get(Coord { x, y });
            }
        }
        new_grid
    }
}

pub fn parse_generic_rle(text: &str) -> Result<Grid<char>, String> {
    let mut header: Option<(usize, usize, String)> = None;
    let mut data = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if header.is_none() && line.starts_with('x') {
            header = Some(parse_rle_header(line)?);
            continue;
        }
        data.push_str(line);
        if line.contains('!') {
            break;
        }
    }

    let (w, h, _rule_str) = match header {
        Some(hd) => hd,
        None => {
            let (w, h) = rle_body_extent(&data);
            (w, h, "B3/S23".to_string())
        }
    };

    if w == 0 || h == 0 {
        return Err("RLE pattern has zero dimension".into());
    }

    let mut cells = vec!['.'; w * h];
    let mut x = 0i32;
    let mut y = 0i32;
    let mut count = 0i32;

    let w_i32 = w as i32;
    let h_i32 = h as i32;

    for c in data.chars() {
        match c {
            '0'..='9' => count = count * 10 + (c as i32 - '0' as i32),
            '$' => {
                y += count.max(1);
                x = 0;
                count = 0;
            }
            '!' => break,
            other if !other.is_whitespace() => {
                let run = count.max(1);
                for _ in 0..run {
                    if x >= w_i32 || y >= h_i32 {
                        return Err(format!(
                            "RLE data exceeds declared dimensions x={w_i32},y={h_i32} at {x},{y}"
                        ));
                    }
                    cells[(y * w_i32 + x) as usize] = other;
                    x += 1;
                }
                count = 0;
            }
            _ => continue,
        }
    }

    Ok(Grid::from_flat(w, cells))
}

/// Infer `(width, height)` of a headerless RLE body by scanning run-lengths:
/// width = furthest column any cell reaches; height = index of the last row
/// containing a cell, +1 (leading blank rows via `$` are preserved).
fn rle_body_extent(data: &str) -> (usize, usize) {
    let (mut x, mut y, mut count, mut max_x, mut max_y) = (0i32, 0i32, 0i32, 0i32, 0i32);
    for c in data.chars() {
        match c {
            '0'..='9' => count = count * 10 + (c as i32 - '0' as i32),
            'b' | 'o' => {
                x += count.max(1);
                max_x = max_x.max(x);
                max_y = max_y.max(y + 1);
                count = 0;
            }
            '$' => {
                y += count.max(1);
                x = 0;
                count = 0;
            }
            '!' => break,
            _ => {}
        }
    }
    (max_x.max(1) as usize, max_y.max(1) as usize)
}

fn parse_rle_header(line: &str) -> Result<(usize, usize, String), String> {
    let mut w = None;
    let mut h = None;
    let mut rule = "B3/S23".to_string(); // default fallback

    for field in line.split(',') {
        let mut kv = field.splitn(2, '=');
        let key = kv.next().unwrap_or("").trim();
        let val = kv.next().unwrap_or("").trim();
        match key {
            "x" => w = val.parse().ok(),
            "y" => h = val.parse().ok(),
            "rule" => rule = val.to_string(),
            _ => {}
        }
    }
    match (w, h) {
        (Some(w), Some(h)) => Ok((w, h, rule)),
        _ => Err(format!("could not parse RLE header: {line:?}")),
    }
}
