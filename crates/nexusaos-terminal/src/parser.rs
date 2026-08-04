//! VT100/ANSI terminal parser backed by `vte`.
//!
//! Provides a `TerminalEmulator` that combines a `vte::Parser` with a
//! `TerminalScreen`, avoiding borrow-checker issues from storing the parser
//! inside the performer.

use std::collections::VecDeque;

use vte::Parser;

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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
        }
    }

    pub fn new_default() -> Self {
        Self::new(TerminalSize::default(), 1000)
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
        for i in 0..self.size.rows - 1 {
            let next = self.cells[i + 1].clone();
            self.cells[i] = next;
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

    fn move_to_next_line(&mut self) {
        self.cursor.0 += 1;
        self.cursor.1 = 0;
        if self.cursor.0 >= self.size.rows {
            self.cursor.0 = self.size.rows - 1;
            self.scroll_up();
        }
    }

    fn write_char(&mut self, c: char) {
        if c == '\n' {
            self.move_to_next_line();
        } else if c == '\r' {
            self.cursor.1 = 0;
        } else {
            let cell = self.current_cell_mut();
            cell.c = c;
            self.cursor.1 += 1;
            if self.cursor.1 >= self.size.cols {
                self.cursor.1 = 0;
                self.move_to_next_line();
            }
        }
    }
}

/// Performer that forwards terminal operations into a shared `TerminalScreen`.
pub struct ScreenPerformer<'a> {
    pub screen: &'a mut TerminalScreen,
}

impl<'a> ScreenPerformer<'a> {
    pub fn new(screen: &'a mut TerminalScreen) -> Self {
        Self { screen }
    }
}

impl<'a> vte::Perform for ScreenPerformer<'a> {
    fn print(&mut self, c: char) {
        self.screen.write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.screen.move_to_next_line(),
            b'\r' => self.screen.cursor.1 = 0,
            b'\t' => {
                self.screen.cursor.1 = (self.screen.cursor.1 + 8) & !7;
                if self.screen.cursor.1 >= self.screen.size.cols {
                    self.screen.cursor.1 = self.screen.size.cols - 1;
                }
            }
            b'\x07' => {}
            b'\x08' if self.screen.cursor.1 > 0 => {
                self.screen.cursor.1 -= 1;
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
                for param in params.iter() {
                    if let Some(&value) = param.first() {
                        match value {
                            0 => {}
                            1 => {}
                            4 => {}
                            30..=37 => {}
                            40..=47 => {}
                            90..=97 => {}
                            100..=107 => {}
                            _ => {}
                        }
                    }
                }
            }
            'A' => {
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.screen.cursor.0 = self.screen.cursor.0.saturating_sub(n);
            }
            'B' => {
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.screen.cursor.0 = (self.screen.cursor.0 + n).min(self.screen.size.rows - 1);
            }
            'C' => {
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.screen.cursor.1 = (self.screen.cursor.1 + n).min(self.screen.size.cols - 1);
            }
            'D' => {
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.screen.cursor.1 = self.screen.cursor.1.saturating_sub(n);
            }
            'J' => {
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(0);
                if n == 0 {
                    for row in &mut self.screen.cells[..self.screen.cursor.0] {
                        for cell in row.iter_mut() {
                            *cell = Cell::new(' ');
                        }
                    }
                    for col in 0..=self.screen.cursor.1 {
                        self.screen.cells[self.screen.cursor.0][col] = Cell::new(' ');
                    }
                } else if n == 1 {
                    for row in &mut self.screen.cells[..self.screen.cursor.0] {
                        for cell in row.iter_mut() {
                            *cell = Cell::new(' ');
                        }
                    }
                    for col in 0..self.screen.cursor.1 {
                        self.screen.cells[self.screen.cursor.0][col] = Cell::new(' ');
                    }
                } else if n == 2 {
                    for row in &mut self.screen.cells {
                        for cell in row.iter_mut() {
                            *cell = Cell::new(' ');
                        }
                    }
                    self.screen.cursor = (0, 0);
                }
            }
            'H' => {
                let mut iter = params.iter();
                let row = iter.next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                let col = iter.next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.screen.cursor.0 = (row.saturating_sub(1)).min(self.screen.size.rows - 1);
                self.screen.cursor.1 = (col.saturating_sub(1)).min(self.screen.size.cols - 1);
            }
            'K' => {
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(0);
                match n {
                    0 => {
                        for col in self.screen.cursor.1..self.screen.size.cols {
                            self.screen.cells[self.screen.cursor.0][col] = Cell::new(' ');
                        }
                    }
                    1 => {
                        for col in 0..self.screen.cursor.1 {
                            self.screen.cells[self.screen.cursor.0][col] = Cell::new(' ');
                        }
                    }
                    2 => {
                        for col in 0..self.screen.size.cols {
                            self.screen.cells[self.screen.cursor.0][col] = Cell::new(' ');
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

/// Terminal emulator that combines a vte parser with a screen.
pub struct TerminalEmulator {
    screen: TerminalScreen,
    parser: Parser,
}

impl TerminalEmulator {
    pub fn new(size: TerminalSize, max_scrollback: usize) -> Self {
        Self { screen: TerminalScreen::new(size, max_scrollback), parser: Parser::new() }
    }

    pub fn new_default() -> Self {
        Self::new(TerminalSize::default(), 1000)
    }

    /// Feed bytes into the parser.
    pub fn feed(&mut self, data: &[u8]) {
        for &byte in data {
            let mut performer = ScreenPerformer::new(&mut self.screen);
            self.parser.advance(&mut performer, byte);
        }
    }

    pub fn screen(&self) -> &TerminalScreen {
        &self.screen
    }

    pub fn screen_mut(&mut self) -> &mut TerminalScreen {
        &mut self.screen
    }

    pub fn lines(&self) -> Vec<String> {
        self.screen.lines()
    }

    pub fn scrollback_lines(&self) -> Vec<String> {
        self.screen.scrollback_lines()
    }

    pub fn resize(&mut self, new_size: TerminalSize) {
        self.screen.resize(new_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_emulator_new() {
        let emulator = TerminalEmulator::new_default();
        assert_eq!(emulator.screen().size.rows, 24);
        assert_eq!(emulator.screen().size.cols, 80);
    }

    #[test]
    fn test_terminal_emulator_print() {
        let mut emulator = TerminalEmulator::new_default();
        emulator.feed(b"Hello");
        let lines = emulator.lines();
        assert!(lines[0].starts_with("Hello"));
    }

    #[test]
    fn test_terminal_emulator_newline() {
        let mut emulator = TerminalEmulator::new(TerminalSize { rows: 2, cols: 5 }, 10);
        emulator.feed(b"Hello\nWorld");
        let lines = emulator.lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains('W') || lines[1].contains('W'));
    }

    #[test]
    fn test_terminal_emulator_newline_simple() {
        let mut emulator = TerminalEmulator::new(TerminalSize { rows: 2, cols: 80 }, 10);
        emulator.feed(b"Hello\nWorld");
        let lines = emulator.lines();
        assert_eq!(lines[0].trim(), "Hello");
        assert_eq!(lines[1].trim(), "World");
    }

    #[test]
    fn test_terminal_emulator_carriage_return() {
        let mut emulator = TerminalEmulator::new_default();
        emulator.feed(b"ABC\rDEF");
        let lines = emulator.lines();
        assert!(lines[0].starts_with("DEF"));
    }

    #[test]
    fn test_terminal_emulator_scroll() {
        let mut emulator = TerminalEmulator::new(TerminalSize { rows: 2, cols: 5 }, 10);
        for i in 0..10 {
            emulator.feed(format!("Line{}\n", i).as_bytes());
        }
        let lines = emulator.lines();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_terminal_emulator_resize() {
        let mut emulator = TerminalEmulator::new_default();
        emulator.resize(TerminalSize { rows: 40, cols: 120 });
        assert_eq!(emulator.screen().size.rows, 40);
        assert_eq!(emulator.screen().size.cols, 120);
    }

    #[test]
    fn test_terminal_emulator_scrollback() {
        let mut emulator = TerminalEmulator::new(TerminalSize { rows: 2, cols: 5 }, 10);
        for i in 0..20 {
            emulator.feed(format!("L{}\n", i).as_bytes());
        }
        assert!(emulator.scrollback_lines().len() > 0);
    }
}
