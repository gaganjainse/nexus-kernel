//! VT100/ANSI terminal parser backed by `vte`.
//!
//! Implements `vte::Perform` to maintain a simple terminal screen state:
//! - A fixed-size grid for the visible area
//! - A scrollback buffer (VecDeque) for lines that scroll off the top
//! - A cursor position

use std::collections::VecDeque;

use vte::{Parser, Perform};

/// Terminal screen dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub rows: usize,
    pub cols: usize,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

/// A single character cell with optional ANSI styling.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub fg: u32,
    pub bg: u32,
    pub bold: bool,
    pub underline: bool,
}

impl Cell {
    pub fn new(c: char) -> Self {
        Self { c, fg: 7, bg: 0, bold: false, underline: false }
    }
}

/// Terminal screen state with scrollback support.
pub struct TerminalScreen {
    size: TerminalSize,
    cursor: (usize, usize),
    cells: Vec<Vec<Cell>>,
    scrollback: VecDeque<Vec<Cell>>,
    max_scrollback: usize,
    parser: Parser,
    current_fg: u32,
    current_bg: u32,
    bold: bool,
    underline: bool,
}

impl TerminalScreen {
    pub fn new(size: TerminalSize, max_scrollback: usize) -> Self {
        let mut cells = Vec::with_capacity(size.rows);
        for _ in 0..size.rows {
            let row = vec![Cell::new(' '); size.cols];
            cells.push(row);
        }
        Self {
            size,
            cursor: (0, 0),
            cells,
            scrollback: VecDeque::with_capacity(max_scrollback),
            max_scrollback,
            parser: Parser::new(),
            current_fg: 7,
            current_bg: 0,
            bold: false,
            underline: false,
        }
    }

    pub fn new_default() -> Self {
        Self::new(TerminalSize::default(), 1000)
    }

    /// Feed bytes into the parser.
    pub fn feed(&mut self, data: &[u8]) {
        for &byte in data {
            self.parser.advance(self, byte);
        }
    }

    /// Get the current screen content as lines of strings.
    pub fn lines(&self) -> Vec<String> {
        self.cells.iter().map(|row| row.iter().map(|c| c.c).collect()).collect()
    }

    /// Get scrollback lines.
    pub fn scrollback_lines(&self) -> Vec<String> {
        self.scrollback.iter().map(|row| row.iter().map(|c| c.c).collect()).collect()
    }

    /// Resize the terminal screen.
    pub fn resize(&mut self, new_size: TerminalSize) {
        if new_size.rows == self.size.rows && new_size.cols == self.size.cols {
            return;
        }
        let mut new_cells = Vec::with_capacity(new_size.rows);
        for r in 0..new_size.rows {
            if r < self.size.rows {
                let mut row = self.cells[r].clone();
                row.resize(new_size.cols, Cell::new(' '));
                new_cells.push(row);
            } else {
                new_cells.push(vec![Cell::new(' '); new_size.cols]);
            }
        }
        self.cells = new_cells;
        self.size = new_size;
        if self.cursor.0 >= new_size.rows {
            self.cursor.0 = new_size.rows - 1;
        }
        if self.cursor.1 >= new_size.cols {
            self.cursor.1 = new_size.cols - 1;
        }
    }

    fn scroll_up(&mut self) {
        let row = std::mem::take(&mut self.cells[0]);
        if self.scrollback.len() >= self.max_scrollback {
            self.scrollback.pop_front();
        }
        self.scrollback.push_back(row);
        for row in &mut self.cells[..self.size.rows - 1] {
            row.copy_from_slice(&self.cells[self.cells.len() - 1]);
        }
        let last = &mut self.cells[self.size.rows - 1];
        for cell in last.iter_mut() {
            *cell = Cell::new(' ');
        }
    }

    fn current_cell_mut(&mut self) -> &mut Cell {
        if self.cursor.0 >= self.size.rows {
            self.cursor.0 = self.size.rows - 1;
        }
        if self.cursor.1 >= self.size.cols {
            self.cursor.1 = self.size.cols - 1;
        }
        &mut self.cells[self.cursor.0][self.cursor.1]
    }
}

impl Perform for TerminalScreen {
    fn print(&mut self, c: char) {
        if c == '\n' {
            self.cursor.0 += 1;
            self.cursor.1 = 0;
            if self.cursor.0 >= self.size.rows {
                self.cursor.0 = self.size.rows - 1;
                self.scroll_up();
            }
        } else if c == '\r' {
            self.cursor.1 = 0;
        } else {
            let cell = self.current_cell_mut();
            cell.c = c;
            cell.fg = self.current_fg;
            cell.bg = self.current_bg;
            cell.bold = self.bold;
            cell.underline = self.underline;
            self.cursor.1 += 1;
            if self.cursor.1 >= self.size.cols {
                self.cursor.1 = 0;
                self.cursor.0 += 1;
                if self.cursor.0 >= self.size.rows {
                    self.cursor.0 = self.size.rows - 1;
                    self.scroll_up();
                }
            }
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.cursor.0 += 1;
                self.cursor.1 = 0;
                if self.cursor.0 >= self.size.rows {
                    self.cursor.0 = self.size.rows - 1;
                    self.scroll_up();
                }
            }
            b'\r' => {
                self.cursor.1 = 0;
            }
            b'\t' => {
                self.cursor.1 = (self.cursor.1 + 8) & !7;
                if self.cursor.1 >= self.size.cols {
                    self.cursor.1 = self.size.cols - 1;
                }
            }
            b'\x07' => {}
            b'\x08' => {
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        match action {
            'm' => {
                let mut iter = params.iter();
                while let Some(param) = iter.next() {
                    match *param {
                        0 => {
                            self.current_fg = 7;
                            self.current_bg = 0;
                            self.bold = false;
                            self.underline = false;
                        }
                        1 => self.bold = true,
                        4 => self.underline = true,
                        30..=37 => self.current_fg = (param - 30) as u32,
                        40..=47 => self.current_bg = (param - 40) as u32,
                        90..=97 => self.current_fg = (param - 90 + 8) as u32,
                        100..=107 => self.current_bg = (param - 100 + 8) as u32,
                        _ => {}
                    }
                }
            }
            'A' => {
                let n = params.first().copied().unwrap_or(1) as usize;
                self.cursor.0 = self.cursor.0.saturating_sub(n);
            }
            'B' => {
                let n = params.first().copied().unwrap_or(1) as usize;
                self.cursor.0 = (self.cursor.0 + n).min(self.size.rows - 1);
            }
            'C' => {
                let n = params.first().copied().unwrap_or(1) as usize;
                self.cursor.1 = (self.cursor.1 + n).min(self.size.cols - 1);
            }
            'D' => {
                let n = params.first().copied().unwrap_or(1) as usize;
                self.cursor.1 = self.cursor.1.saturating_sub(n);
            }
            'J' => {
                let n = params.first().copied().unwrap_or(0);
                if n == 0 {
                    for row in &mut self.cells[..self.cursor.0] {
                        for cell in row.iter_mut() {
                            *cell = Cell::new(' ');
                        }
                    }
                    for col in 0..=self.cursor.1 {
                        self.cells[self.cursor.0][col] = Cell::new(' ');
                    }
                } else if n == 1 {
                    for row in &mut self.cells[..self.cursor.0] {
                        for cell in row.iter_mut() {
                            *cell = Cell::new(' ');
                        }
                    }
                    for col in 0..self.cursor.1 {
                        self.cells[self.cursor.0][col] = Cell::new(' ');
                    }
                } else if n == 2 {
                    for row in &mut self.cells {
                        for cell in row.iter_mut() {
                            *cell = Cell::new(' ');
                        }
                    }
                    self.cursor = (0, 0);
                }
            }
            'H' => {
                let mut iter = params.iter();
                let row = iter.next().copied().unwrap_or(1) as usize;
                let col = iter.next().copied().unwrap_or(1) as usize;
                self.cursor.0 = (row.saturating_sub(1)).min(self.size.rows - 1);
                self.cursor.1 = (col.saturating_sub(1)).min(self.size.cols - 1);
            }
            'K' => {
                let n = params.first().copied().unwrap_or(0);
                match n {
                    0 => {
                        for col in self.cursor.1..self.size.cols {
                            self.cells[self.cursor.0][col] = Cell::new(' ');
                        }
                    }
                    1 => {
                        for col in 0..self.cursor.1 {
                            self.cells[self.cursor.0][col] = Cell::new(' ');
                        }
                    }
                    2 => {
                        for col in 0..self.size.cols {
                            self.cells[self.cursor.0][col] = Cell::new(' ');
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_screen_new() {
        let screen = TerminalScreen::new_default();
        assert_eq!(screen.size.rows, 24);
        assert_eq!(screen.size.cols, 80);
        assert_eq!(screen.cursor, (0, 0));
    }

    #[test]
    fn test_terminal_screen_print() {
        let mut screen = TerminalScreen::new_default();
        screen.feed(b"Hello");
        let lines = screen.lines();
        assert!(lines[0].starts_with("Hello"));
    }

    #[test]
    fn test_terminal_screen_newline() {
        let mut screen = TerminalScreen::new(TerminalSize { rows: 2, cols: 5 }, 10);
        screen.feed(b"Hello\nWorld");
        let lines = screen.lines();
        assert_eq!(lines[0], "Hello");
        assert_eq!(lines[1], "World");
    }

    #[test]
    fn test_terminal_screen_carriage_return() {
        let mut screen = TerminalScreen::new_default();
        screen.feed(b"ABC\rDEF");
        let lines = screen.lines();
        assert!(lines[0].starts_with("DEF"));
    }

    #[test]
    fn test_terminal_screen_scroll() {
        let mut screen = TerminalScreen::new(TerminalSize { rows: 2, cols: 5 }, 10);
        for i in 0..10 {
            screen.feed(format!("Line{}\n", i).as_bytes());
        }
        let lines = screen.lines();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_terminal_screen_resize() {
        let mut screen = TerminalScreen::new_default();
        screen.resize(TerminalSize { rows: 40, cols: 120 });
        assert_eq!(screen.size.rows, 40);
        assert_eq!(screen.size.cols, 120);
    }

    #[test]
    fn test_terminal_screen_scrollback() {
        let mut screen = TerminalScreen::new(TerminalSize { rows: 2, cols: 5 }, 10);
        for i in 0..20 {
            screen.feed(format!("L{}\n", i).as_bytes());
        }
        assert!(screen.scrollback_lines().len() > 0);
    }
}
