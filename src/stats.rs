use crate::{basic_grids::Grid, board::BoardWindow, ca::CACell};

impl BoardWindow {
    pub fn get_all_boards_stats(&self) -> Option<Grid<(u32, u32)>> {
        let nboard = self.get_num_valid_boards()?;
        let mut board_liv = Grid::from_rect(self.rect, 0);
        let mut board = Grid::from_rect(self.rect, CACell::DEAD);
        for i in 0..nboard {
            self.write_to_grid(i, &mut board, false)?;
            for y in self.rect.top..self.rect.bottom {
                for x in self.rect.left..self.rect.right {
                    if *board.get((x, y).into()) == CACell::ALIVE {
                        *board_liv.get_mut((x, y).into()) += 1;
                    }
                }
            }
        }
        let mut board = Grid::from_rect(self.rect, (0, 0));
        for y in self.rect.top..self.rect.bottom {
            for x in self.rect.left..self.rect.right {
                *board.get_mut((x, y).into()) = (
                    *board_liv.get((x, y).into()),
                    nboard as u32 - *board_liv.get((x, y).into()),
                );
            }
        }
        Some(board)
    }
}

pub fn print_viz_fracs(s: &Grid<(u32, u32)>) {
    for row in s.data.chunks(s.size.width() as usize) {
        for &(ali, ded) in row {
            if ali == 0 {
                // Fully dead - blue
                print!("\x1b[44m  \x1b[0m");
            } else if ded == 0 {
                // Fully alive - red
                print!("\x1b[41m  \x1b[0m");
            } else {
                let smoll = ali.min(ded) as f64;
                let tot = (ali + ded) as f64;
                let intensity = (smoll / tot).log10() / -6.0 * 255.0;
                if ali > ded {
                    print!("\x1b[48;2;{0};{0};0m  \x1b[0m", intensity as u8);
                } else {
                    print!("\x1b[48;2;0;{0};{0}m  \x1b[0m", intensity as u8);
                }
            }
        }
        println!(); // Newline at the end of each row
    }
}
