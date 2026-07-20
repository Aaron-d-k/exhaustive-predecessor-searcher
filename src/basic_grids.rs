use bitflags::bitflags;

#[derive(Clone)]
pub struct Grid<T> {
    pub data: Vec<T>,
    pub size: Rect,
}

impl<T: Clone> Grid<T> {
    pub fn new(height: i32, width: i32, default: T) -> Self {
        Self {
            data: vec![default; (width as usize) * (height as usize)],
            size: Rect {
                top: 0,
                left: 0,
                bottom: height,
                right: width,
            },
        }
    }

    pub fn from_rect(rect: Rect, default: T) -> Self {
        Self {
            data: vec![default; (rect.width() as usize) * (rect.height() as usize)],
            size: rect,
        }
    }
}

impl<T> Grid<T> {
    pub fn from_flat(width: usize, flat: Vec<T>) -> Self {
        assert_eq!(flat.len() % width, 0);
        Self {
            size: Rect {
                top: 0,
                left: 0,
                bottom: (flat.len() / width) as i32,
                right: width as i32,
            },
            data: flat,
        }
    }

    pub fn get(&self, p: Coord) -> &T {
        debug_assert!(self.size.is_inside(p));
        &self.data[((p.y - self.size.top) * self.size.width() + (p.x - self.size.left)) as usize]
    }

    pub fn get_mut(&mut self, p: Coord) -> &mut T {
        debug_assert!(self.size.is_inside(p));
        &mut self.data
            [((p.y - self.size.top) * self.size.width() + (p.x - self.size.left)) as usize]
    }

    pub fn set_origin(&mut self, p: Coord) {
        self.size = Rect {
            top: p.y,
            left: p.x,
            bottom: p.y + self.size.height(),
            right: p.x + self.size.width(),
        }
    }

    pub fn rotate_90_cw_inplace(&mut self) {
        let w = self.size.width() as usize;
        let h = self.size.height() as usize;

        if w > 0 && h > 0 {
            let mut old_data: Vec<Option<T>> = std::mem::take(&mut self.data)
                .into_iter()
                .map(Some)
                .collect();

            let mut new_data = Vec::with_capacity(w * h);

            let new_w = h;
            let new_h = w;

            // In a 90-deg clockwise rotation, the new (x, y) maps to the old (y, h - 1 - x).
            for new_y in 0..new_h {
                for new_x in 0..new_w {
                    let old_x = new_y;
                    let old_y = h - 1 - new_x;
                    new_data.push(old_data[old_y * w + old_x].take().unwrap());
                }
            }

            self.data = new_data;
        }

        // Swap dimensions for the 90-degree rotation
        let old_w = self.size.right - self.size.left;
        let old_h = self.size.bottom - self.size.top;

        self.size.right = self.size.left + old_h;
        self.size.bottom = self.size.top + old_w;
    }

    pub fn into_rows_mut(&mut self) -> Vec<&mut [T]> {
        self.data.chunks_mut(self.size.width() as usize).collect()
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U) -> Grid<U> {
        Grid {
            data: self.data.iter().map(f).collect(),
            size: self.size,
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Directions : u8 {
        const Up = 0b0001;
        const Down = 0b0010;
        const Left = 0b0100;
        const Right = 0b1000;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
}

impl Direction {
    pub fn opposite(self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }

    pub fn rotate_90_cw(self) -> Direction {
        match self {
            Direction::Up => Direction::Right,
            Direction::Down => Direction::Left,
            Direction::Left => Direction::Up,
            Direction::Right => Direction::Down,
        }
    }
}

impl From<Direction> for Directions {
    fn from(dir: Direction) -> Self {
        match dir {
            Direction::Up => Directions::Up,
            Direction::Down => Directions::Down,
            Direction::Left => Directions::Left,
            Direction::Right => Directions::Right,
        }
    }
}

impl Directions {
    pub fn to_single(self) -> Option<Direction> {
        match self {
            Directions::Up => Some(Direction::Up),
            Directions::Down => Some(Direction::Down),
            Directions::Left => Some(Direction::Left),
            Directions::Right => Some(Direction::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
}

impl From<(i32, i32)> for Coord {
    fn from(value: (i32, i32)) -> Self {
        Coord {
            x: value.0,
            y: value.1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rect {
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
}

impl Rect {
    pub fn new(top: i32, left: i32, bottom: i32, right: i32) -> Self {
        debug_assert!(top < bottom);
        debug_assert!(left < right);
        Rect {
            top,
            left,
            bottom,
            right,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn trim(&self, direction: Direction, distance: i32) -> Rect {
        match direction {
            Direction::Up => Rect::new(self.top + distance, self.left, self.bottom, self.right),
            Direction::Down => Rect::new(self.top, self.left, self.bottom - distance, self.right),
            Direction::Left => Rect::new(self.top, self.left + distance, self.bottom, self.right),
            Direction::Right => Rect::new(self.top, self.left, self.bottom, self.right - distance),
        }
    }

    pub fn is_inside(&self, p: Coord) -> bool {
        self.top <= p.y && p.y < self.bottom && self.left <= p.x && p.x < self.right
    }
}

pub type PackedCellsUnderlying = u128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackedCells(pub PackedCellsUnderlying);

impl PackedCells {
    pub const ALL_DEAD: PackedCells = PackedCells(0);

    pub fn trim(self, r_old: Rect, to_trim: Direction, distance: i32) -> PackedCells {
        debug_assert!(r_old.width() * r_old.height() < PackedCellsUnderlying::BITS as i32);
        debug_assert_eq!(self.0 >> (r_old.width() * r_old.height()), 0);
        debug_assert!(distance >= 0);

        match to_trim {
            Direction::Down => {
                debug_assert!(distance < r_old.height());
                let new_size = r_old.width() * (r_old.height() - distance);
                PackedCells(self.0 & (((1 as PackedCellsUnderlying) << new_size) - 1))
            }
            Direction::Up => {
                debug_assert!(distance < r_old.height());
                PackedCells(self.0 >> (r_old.width() * distance))
            }
            Direction::Right => {
                debug_assert!(distance < r_old.width());
                let new_width = r_old.width() - distance;
                let mut result = 0;
                for y in 0..r_old.height() {
                    let row = (self.0 >> (y * r_old.width()))
                        & (((1 as PackedCellsUnderlying) << r_old.width()) - 1);
                    let trimmed_row = row & (((1 as PackedCellsUnderlying) << new_width) - 1);
                    result |= trimmed_row << (y * new_width);
                }
                PackedCells(result)
            }
            Direction::Left => {
                debug_assert!(distance < r_old.width());
                let new_width = r_old.width() - distance;
                let mut result = 0;
                for y in 0..r_old.height() {
                    let row = (self.0 >> (y * r_old.width()))
                        & (((1 as PackedCellsUnderlying) << r_old.width()) - 1);
                    let trimmed_row = row >> distance;
                    result |= trimmed_row << (y * new_width);
                }
                PackedCells(result)
            }
        }
    }

    pub fn join(self, c2: PackedCells, r1: Rect, r2: Rect, join_direc: Direction) -> PackedCells {
        debug_assert!(r1.width() * r1.height() < PackedCellsUnderlying::BITS as i32);
        debug_assert!(r2.width() * r2.height() < PackedCellsUnderlying::BITS as i32);
        debug_assert_eq!(self.0 >> r1.width() * r1.height(), 0);
        debug_assert_eq!(c2.0 >> r2.width() * r2.height(), 0);

        match join_direc {
            Direction::Up => {
                // c2 is above self
                debug_assert_eq!(r1.top, r2.bottom);
                debug_assert_eq!(r1.left, r2.left);
                debug_assert_eq!(r1.right, r2.right);
                debug_assert!(
                    (r1.width() * (r1.height() + r2.height())) < PackedCellsUnderlying::BITS as i32
                );

                PackedCells((self.0 << (r2.width() * r2.height())) | c2.0)
            }
            Direction::Down => {
                // c2 is below self
                debug_assert_eq!(r1.bottom, r2.top);
                debug_assert_eq!(r1.left, r2.left);
                debug_assert_eq!(r1.right, r2.right);
                debug_assert!(
                    (r1.width() * (r1.height() + r2.height())) < PackedCellsUnderlying::BITS as i32
                );

                PackedCells((c2.0 << (r1.width() * r1.height())) | self.0)
            }
            Direction::Left => {
                // c2 is to the left of self
                debug_assert_eq!(r1.left, r2.right);
                debug_assert_eq!(r1.top, r2.top);
                debug_assert_eq!(r1.bottom, r2.bottom);
                debug_assert!(
                    ((r1.width() + r2.width()) * r1.height()) < PackedCellsUnderlying::BITS as i32
                );

                let h = r1.height();
                let w1 = r1.width();
                let w2 = r2.width();
                let dest_w = w1 + w2;

                let mask1 = ((1 as PackedCellsUnderlying) << w1) - 1;
                let mask2 = ((1 as PackedCellsUnderlying) << w2) - 1;
                let mut result = 0;

                for y in 0..h {
                    let row1 = (self.0 >> (y * w1)) & mask1;
                    let row2 = (c2.0 >> (y * w2)) & mask2;

                    // c2 comes first (lower x coordinates)
                    let combined_row = row2 | (row1 << w2);
                    result |= combined_row << (y * dest_w);
                }

                PackedCells(result)
            }
            Direction::Right => {
                // c2 is to the right of self
                debug_assert_eq!(r1.right, r2.left);
                debug_assert_eq!(r1.top, r2.top);
                debug_assert_eq!(r1.bottom, r2.bottom);
                debug_assert!(
                    ((r1.width() + r2.width()) * r1.height()) < PackedCellsUnderlying::BITS as i32
                );

                let h = r1.height();
                let w1 = r1.width();
                let w2 = r2.width();
                let dest_w = w1 + w2;

                let mask1 = ((1 as PackedCellsUnderlying) << w1) - 1;
                let mask2 = ((1 as PackedCellsUnderlying) << w2) - 1;
                let mut result = 0;

                for y in 0..h {
                    let row1 = (self.0 >> (y * w1)) & mask1;
                    let row2 = (c2.0 >> (y * w2)) & mask2;

                    // self comes first (lower x coordinates)
                    let combined_row = row1 | (row2 << w1);
                    result |= combined_row << (y * dest_w);
                }

                PackedCells(result)
            }
        }
    }
}
