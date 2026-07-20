use std::collections::VecDeque;
use std::io::{Read, Write};
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;

use crate::TERMINAL_FONT;
use devhub_core::{shell_quote, validate_remote_path, validate_ssh_host, Project, ProjectSource};
use gpui::prelude::*;
use gpui::*;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

const MAX_SCROLLBACK_LINES: usize = 1_000;

type TerminalProcess = (
    Box<dyn Read + Send>,
    Box<dyn Write + Send>,
    Box<dyn MasterPty + Send>,
    Box<dyn Child + Send + Sync>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalLaunch {
    Local { cwd: PathBuf },
    Remote { host: String, cwd: String },
}

impl TerminalLaunch {
    pub fn for_project(project: &Project) -> Result<Self, String> {
        match &project.source {
            ProjectSource::Local => Ok(Self::Local {
                cwd: project.path.clone(),
            }),
            ProjectSource::Remote { host, .. } => Ok(Self::Remote {
                host: validate_ssh_host(host)?,
                cwd: validate_remote_path(&project.path.to_string_lossy())?,
            }),
        }
    }

    fn command(&self) -> Result<(CommandBuilder, String), String> {
        match self {
            Self::Local { cwd } => {
                let (mut command, label) = default_shell_command()?;
                command.cwd(cwd);
                Ok((command, label))
            }
            Self::Remote { host, cwd } => {
                let script = format!(
                    "cd -- {} && exec \"${{SHELL:-/bin/sh}}\" -l",
                    shell_quote(cwd)
                );
                let mut command = CommandBuilder::new("ssh");
                command.args(["-tt", host, &script]);
                Ok((command, format!("ssh {host}")))
            }
        }
    }

    pub fn cwd_label(&self) -> String {
        match self {
            Self::Local { cwd } => cwd.display().to_string(),
            Self::Remote { host, cwd } => format!("{host}:{cwd}"),
        }
    }
}

#[cfg(not(windows))]
fn default_shell_command() -> Result<(CommandBuilder, String), String> {
    let command = CommandBuilder::new_default_prog();
    let label = command.get_shell();
    Ok((command, label))
}

#[cfg(windows)]
fn default_shell_command() -> Result<(CommandBuilder, String), String> {
    let explicit = std::env::var_os("DEVHUB_SHELL").or_else(|| std::env::var_os("SHELL"));
    let program = explicit
        .as_deref()
        .and_then(resolve_executable)
        .or_else(|| resolve_executable("pwsh.exe".as_ref()))
        .or_else(|| resolve_executable("powershell.exe".as_ref()))
        .or_else(|| resolve_executable("cmd.exe".as_ref()))
        .ok_or_else(|| {
            "No interactive shell was found. Set DEVHUB_SHELL to a shell executable.".to_string()
        })?;
    let label = program
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Terminal")
        .to_string();
    Ok((CommandBuilder::new(program), label))
}

#[cfg(windows)]
fn resolve_executable(program: &std::ffi::OsStr) -> Option<PathBuf> {
    let program = Path::new(program);
    if program.components().count() > 1 {
        return program.is_file().then(|| program.to_path_buf());
    }

    let extensions = std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![".EXE".to_string()]);
    let mut matches = Vec::new();
    for directory in std::env::split_paths(&std::env::var_os("PATH")?) {
        let direct = directory.join(program);
        if direct.is_file() {
            matches.push(direct);
            continue;
        }
        if program.extension().is_none() {
            for extension in &extensions {
                let candidate =
                    directory.join(format!("{}{}", program.to_string_lossy(), extension));
                if candidate.is_file() {
                    matches.push(candidate);
                    break;
                }
            }
        }
    }

    matches
        .iter()
        .find(|path| {
            !path
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("\\windowsapps\\")
        })
        .cloned()
        .or_else(|| matches.into_iter().next())
}

#[derive(Clone)]
struct TerminalCell {
    text: String,
    fg: Hsla,
    bg: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
}

impl TerminalCell {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fg: Hsla::default(),
            bg: Hsla::default(),
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
        }
    }
}

#[derive(Clone)]
struct TerminalState {
    screen: Vec<Vec<TerminalCell>>,
    scrollback: VecDeque<Vec<TerminalCell>>,
    cursor_row: usize,
    cursor_col: usize,
    cols: usize,
    rows_count: usize,
    default_fg: Hsla,
    default_bg: Hsla,
    fg: Hsla,
    bg: Hsla,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
    strikethrough: bool,
    saved_cursor_row: usize,
    saved_cursor_col: usize,
    saved_fg: Hsla,
    saved_bg: Hsla,
    saved_bold: bool,
    saved_dim: bool,
    saved_italic: bool,
    saved_underline: bool,
    saved_inverse: bool,
    saved_strikethrough: bool,
    responses: Vec<Vec<u8>>,
}

impl TerminalState {
    fn new(cols: usize, rows: usize, default_fg: Hsla, default_bg: Hsla) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            screen: vec![Self::blank_row(cols, default_fg, default_bg); rows],
            scrollback: VecDeque::new(),
            cursor_row: 0,
            cursor_col: 0,
            cols,
            rows_count: rows,
            default_fg,
            default_bg,
            fg: default_fg,
            bg: default_bg,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
            strikethrough: false,
            saved_cursor_row: 0,
            saved_cursor_col: 0,
            saved_fg: default_fg,
            saved_bg: default_bg,
            saved_bold: false,
            saved_dim: false,
            saved_italic: false,
            saved_underline: false,
            saved_inverse: false,
            saved_strikethrough: false,
            responses: Vec::new(),
        }
    }

    fn blank_cell(default_fg: Hsla, default_bg: Hsla) -> TerminalCell {
        let mut cell = TerminalCell::plain(" ");
        cell.fg = default_fg;
        cell.bg = default_bg;
        cell
    }

    fn blank_row(cols: usize, default_fg: Hsla, default_bg: Hsla) -> Vec<TerminalCell> {
        vec![Self::blank_cell(default_fg, default_bg); cols]
    }

    fn new_blank_row(&self) -> Vec<TerminalCell> {
        Self::blank_row(self.cols, self.default_fg, self.default_bg)
    }

    fn line_feed(&mut self) {
        if self.cursor_row + 1 >= self.rows_count {
            self.scroll_screen(1);
        } else {
            self.cursor_row += 1;
        }
    }

    fn scroll_screen(&mut self, count: usize) {
        for _ in 0..count {
            if !self.screen.is_empty() {
                self.scrollback.push_back(self.screen.remove(0));
            }
            while self.scrollback.len() > MAX_SCROLLBACK_LINES {
                self.scrollback.pop_front();
            }
            self.screen.push(self.new_blank_row());
        }
        self.cursor_row = self.cursor_row.min(self.rows_count.saturating_sub(1));
    }

    fn put_char(&mut self, ch: char) {
        if ch == '\n' {
            self.line_feed();
        } else if ch == '\r' {
            self.cursor_col = 0;
        } else {
            if self.cursor_col >= self.cols {
                self.cursor_col = 0;
                self.line_feed();
            }
            let mut cell = TerminalCell::plain(ch.to_string());
            let (mut fg, bg) = if self.inverse {
                (self.bg, self.fg)
            } else {
                (self.fg, self.bg)
            };
            if self.dim {
                fg.a *= 0.65;
            }
            cell.fg = fg;
            cell.bg = bg;
            cell.bold = self.bold;
            cell.italic = self.italic;
            cell.underline = self.underline;
            cell.strikethrough = self.strikethrough;
            self.screen[self.cursor_row][self.cursor_col] = cell;
            self.cursor_col += 1;
        }
    }

    fn move_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(self.rows_count.saturating_sub(1));
        self.cursor_col = col.min(self.cols.saturating_sub(1));
    }

    fn clear_line(&mut self, mode: u8) {
        let blank = Self::blank_cell(self.default_fg, self.default_bg);
        let row = &mut self.screen[self.cursor_row];
        match mode {
            0 => {
                for cell in row.iter_mut().skip(self.cursor_col) {
                    *cell = blank.clone();
                }
            }
            1 => {
                for cell in row.iter_mut().take(self.cursor_col + 1) {
                    *cell = blank.clone();
                }
            }
            2 => {
                for cell in row.iter_mut() {
                    *cell = blank.clone();
                }
            }
            _ => {}
        }
    }

    fn clear_screen(&mut self, mode: u8) {
        let blank = Self::blank_cell(self.default_fg, self.default_bg);
        match mode {
            0 => {
                for row in self.screen.iter_mut().skip(self.cursor_row + 1) {
                    for cell in row.iter_mut() {
                        *cell = blank.clone();
                    }
                }
                self.clear_line(0);
            }
            1 => {
                for row in self.screen.iter_mut().take(self.cursor_row) {
                    for cell in row.iter_mut() {
                        *cell = blank.clone();
                    }
                }
                self.clear_line(1);
            }
            2 => {
                for row in self.screen.iter_mut() {
                    for cell in row.iter_mut() {
                        *cell = blank.clone();
                    }
                }
                self.move_cursor(0, 0);
            }
            3 => self.scrollback.clear(),
            _ => {}
        }
    }

    fn rendered_rows(&self) -> impl Iterator<Item = (usize, &Vec<TerminalCell>)> {
        self.scrollback.iter().chain(self.screen.iter()).enumerate()
    }

    fn cursor_render_row(&self) -> usize {
        self.scrollback.len() + self.cursor_row
    }

    fn take_responses(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.responses)
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        for row in self.scrollback.iter_mut().chain(self.screen.iter_mut()) {
            row.resize(cols, Self::blank_cell(self.default_fg, self.default_bg));
        }

        if rows > self.rows_count {
            self.screen.extend(
                (0..rows - self.rows_count)
                    .map(|_| Self::blank_row(cols, self.default_fg, self.default_bg)),
            );
        } else if rows < self.rows_count {
            let remove = self.rows_count - rows;
            for _ in 0..remove.min(self.screen.len().saturating_sub(1)) {
                self.scrollback.push_back(self.screen.remove(0));
            }
            while self.scrollback.len() > MAX_SCROLLBACK_LINES {
                self.scrollback.pop_front();
            }
        }

        self.cols = cols;
        self.rows_count = rows;
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
    }
}

impl vte::Perform for TerminalState {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.put_char('\n'),
            b'\r' => self.put_char('\r'),
            b'\t' => {
                for _ in 0..4 {
                    self.put_char(' ');
                }
            }
            b'\x08' => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
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
            'm' => self.apply_sgr(params),
            'A' => {
                let n = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                self.move_cursor(self.cursor_row.saturating_sub(n), self.cursor_col);
            }
            'B' => {
                let n = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                self.move_cursor(self.cursor_row + n, self.cursor_col);
            }
            'C' => {
                let n = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                self.move_cursor(self.cursor_row, self.cursor_col + n);
            }
            'D' => {
                let n = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                self.move_cursor(self.cursor_row, self.cursor_col.saturating_sub(n));
            }
            'H' | 'f' => {
                let row = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1)
                    .max(1) as usize
                    - 1;
                let col = params
                    .iter()
                    .nth(1)
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1)
                    .max(1) as usize
                    - 1;
                self.move_cursor(row, col);
            }
            'J' => {
                let mode = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(0);
                self.clear_screen(mode as u8);
            }
            'K' => {
                let mode = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(0);
                self.clear_line(mode as u8);
            }
            'L' => {
                let n = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                for _ in 0..n {
                    if self.cursor_row < self.rows_count {
                        let blank_row = self.new_blank_row();
                        self.screen.insert(self.cursor_row, blank_row);
                        self.screen.truncate(self.rows_count);
                    }
                }
            }
            'M' => {
                let n = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                for _ in 0..n {
                    if self.cursor_row < self.screen.len() && !self.screen.is_empty() {
                        self.screen.remove(self.cursor_row);
                        self.screen.push(self.new_blank_row());
                    }
                }
            }
            'S' => {
                let n = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                self.scroll_screen(n);
            }
            'T' => {
                let n = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                for _ in 0..n {
                    if self.cursor_row > 0 {
                        let blank_row = self.new_blank_row();
                        self.screen.insert(self.cursor_row, blank_row);
                        self.screen.truncate(self.rows_count);
                        self.cursor_row = self.cursor_row.saturating_sub(1);
                    }
                }
            }
            's' => {
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
                self.saved_fg = self.fg;
                self.saved_bg = self.bg;
                self.saved_bold = self.bold;
                self.saved_dim = self.dim;
                self.saved_italic = self.italic;
                self.saved_underline = self.underline;
                self.saved_inverse = self.inverse;
                self.saved_strikethrough = self.strikethrough;
            }
            'u' => {
                self.cursor_row = self.saved_cursor_row;
                self.cursor_col = self.saved_cursor_col;
                self.fg = self.saved_fg;
                self.bg = self.saved_bg;
                self.bold = self.saved_bold;
                self.dim = self.saved_dim;
                self.italic = self.saved_italic;
                self.underline = self.saved_underline;
                self.inverse = self.saved_inverse;
                self.strikethrough = self.saved_strikethrough;
                self.move_cursor(self.cursor_row, self.cursor_col);
            }
            'n' => {
                let query = params
                    .iter()
                    .next()
                    .and_then(|param| param.first())
                    .copied()
                    .unwrap_or(0);
                match query {
                    5 => self.responses.push(b"\x1b[0n".to_vec()),
                    6 => self.responses.push(
                        format!("\x1b[{};{}R", self.cursor_row + 1, self.cursor_col + 1)
                            .into_bytes(),
                    ),
                    _ => {}
                }
            }
            'c' => self.responses.push(b"\x1b[?1;2c".to_vec()),
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}
}

impl TerminalState {
    fn default_fg(&self) -> Hsla {
        self.default_fg
    }

    fn default_bg(&self) -> Hsla {
        self.default_bg
    }

    fn reset_attributes(&mut self) {
        self.fg = self.default_fg();
        self.bg = self.default_bg();
        self.bold = false;
        self.dim = false;
        self.italic = false;
        self.underline = false;
        self.inverse = false;
        self.strikethrough = false;
    }

    fn apply_sgr(&mut self, params: &vte::Params) {
        let params = params.iter().collect::<Vec<_>>();
        if params.is_empty() {
            self.reset_attributes();
            return;
        }

        let mut index = 0;
        while index < params.len() {
            let value = params[index].first().copied().unwrap_or(0);
            match value {
                0 => self.reset_attributes(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => self.underline = true,
                7 => self.inverse = true,
                9 => self.strikethrough = true,
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                23 => self.italic = false,
                24 => self.underline = false,
                27 => self.inverse = false,
                29 => self.strikethrough = false,
                30..=37 => self.fg = self.ansi_color(value - 30),
                38 => {
                    let (color, consumed) = self.parse_extended_color(&params, index);
                    if let Some(color) = color {
                        self.fg = color;
                    }
                    index += consumed;
                }
                39 => self.fg = self.default_fg(),
                40..=47 => self.bg = self.ansi_color(value - 40),
                48 => {
                    let (color, consumed) = self.parse_extended_color(&params, index);
                    if let Some(color) = color {
                        self.bg = color;
                    }
                    index += consumed;
                }
                49 => self.bg = self.default_bg(),
                90..=97 => self.fg = self.ansi_color(value - 82),
                100..=107 => self.bg = self.ansi_color(value - 92),
                _ => {}
            }
            index += 1;
        }
    }

    fn parse_extended_color(&self, params: &[&[u16]], index: usize) -> (Option<Hsla>, usize) {
        let current = params[index];
        if current.len() > 1 {
            let spec = &current[1..];
            return match spec.first().copied() {
                Some(5) => (spec.get(1).map(|index| self.indexed_color(*index)), 0),
                Some(2) if spec.len() >= 4 => {
                    let rgb = &spec[spec.len() - 3..];
                    (Some(Self::rgb_color(rgb[0], rgb[1], rgb[2])), 0)
                }
                _ => (None, 0),
            };
        }

        let mode = params
            .get(index + 1)
            .and_then(|param| param.first())
            .copied();
        match mode {
            Some(5) => (
                params
                    .get(index + 2)
                    .and_then(|param| param.first())
                    .map(|index| self.indexed_color(*index)),
                2,
            ),
            Some(2) => {
                let red = params
                    .get(index + 2)
                    .and_then(|param| param.first())
                    .copied();
                let green = params
                    .get(index + 3)
                    .and_then(|param| param.first())
                    .copied();
                let blue = params
                    .get(index + 4)
                    .and_then(|param| param.first())
                    .copied();
                (
                    red.zip(green)
                        .zip(blue)
                        .map(|((r, g), b)| Self::rgb_color(r, g, b)),
                    4,
                )
            }
            _ => (None, usize::from(mode.is_some())),
        }
    }

    fn rgb_color(r: u16, g: u16, b: u16) -> Hsla {
        Self::rgb_to_hsla(
            r.min(255) as f32 / 255.0,
            g.min(255) as f32 / 255.0,
            b.min(255) as f32 / 255.0,
        )
    }

    fn rgb_to_hsla(r: f32, g: f32, b: f32) -> Hsla {
        let max = r.max(g.max(b));
        let min = r.min(g.min(b));
        let l = (max + min) / 2.0;
        if max == min {
            Hsla {
                h: 0.0,
                s: 0.0,
                l,
                a: 1.0,
            }
        } else {
            let d = max - min;
            let s = if l > 0.5 {
                d / (2.0 - max - min)
            } else {
                d / (max + min)
            };
            let h = if max == r {
                ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
            } else if max == g {
                ((b - r) / d + 2.0) / 6.0
            } else {
                ((r - g) / d + 4.0) / 6.0
            };
            Hsla { h, s, l, a: 1.0 }
        }
    }

    fn ansi_color(&self, code: u16) -> Hsla {
        const DARK: [u32; 16] = [
            0x1b1d23, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
            0x5c6370, 0xff7a85, 0xb3e37b, 0xffcc80, 0x75bfff, 0xd291f0, 0x68d4df, 0xf4f5f7,
        ];
        const LIGHT: [u32; 16] = [
            0x2b2d31, 0xb4232c, 0x287a45, 0x8a5a00, 0x245fb5, 0x7c3fa1, 0x0b7285, 0x4a4d53,
            0x686b72, 0xd0303a, 0x2f8a50, 0xa66a00, 0x2e6dcc, 0x914ab8, 0x0d8295, 0x111216,
        ];
        let palette = if self.default_bg.l < 0.5 { DARK } else { LIGHT };
        let color = palette[usize::from(code.min(15))];
        Self::rgb_color(
            ((color >> 16) & 0xff) as u16,
            ((color >> 8) & 0xff) as u16,
            (color & 0xff) as u16,
        )
    }

    fn indexed_color(&self, index: u16) -> Hsla {
        let index = index.min(255);
        match index {
            0..=15 => self.ansi_color(index),
            16..=231 => {
                const LEVELS: [u16; 6] = [0, 95, 135, 175, 215, 255];
                let index = index - 16;
                let red = LEVELS[usize::from(index / 36)];
                let green = LEVELS[usize::from((index % 36) / 6)];
                let blue = LEVELS[usize::from(index % 6)];
                Self::rgb_color(red, green, blue)
            }
            _ => {
                let gray = (index - 232) * 10 + 8;
                Self::rgb_color(gray, gray, gray)
            }
        }
    }
}

pub struct TerminalPanel {
    focus_handle: FocusHandle,
    state: TerminalState,
    parser: vte::Parser,
    writer: Option<Box<dyn Write + Send>>,
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    _reader_task: Task<()>,
    scroll_handle: ScrollHandle,
    default_fg: Hsla,
    default_bg: Hsla,
    pub shell: String,
    pub cwd_label: String,
    pub spawn_error: Option<String>,
    pub process_exited: bool,
}

impl Drop for TerminalPanel {
    fn drop(&mut self) {
        self.writer.take();
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

impl TerminalPanel {
    pub fn new(
        cx: &mut Context<Self>,
        launch: TerminalLaunch,
        cols: usize,
        rows: usize,
        default_fg: Hsla,
        default_bg: Hsla,
    ) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let state = TerminalState::new(cols, rows, default_fg, default_bg);
        let parser = vte::Parser::new();
        let cwd_label = launch.cwd_label();

        let spawn_result = Self::spawn_process(&launch, cols, rows);
        let (writer, master, child, reader_task, shell, spawn_error) = match spawn_result {
            Ok(((mut reader, writer, master, child), shell)) => {
                enum ReaderMessage {
                    Data(Vec<u8>),
                    Eof,
                    Error(String),
                }

                let (tx, rx) = std::sync::mpsc::sync_channel::<ReaderMessage>(256);

                std::thread::spawn(move || {
                    let mut buf = [0u8; 4096];
                    loop {
                        match reader.read(&mut buf) {
                            Ok(0) => {
                                let _ = tx.send(ReaderMessage::Eof);
                                break;
                            }
                            Err(error) => {
                                let _ = tx.send(ReaderMessage::Error(error.to_string()));
                                break;
                            }
                            Ok(n) => {
                                if tx.send(ReaderMessage::Data(buf[..n].to_vec())).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });

                let task = cx.spawn(async move |this, cx| loop {
                    let mut exited = false;
                    let mut read_error = None;
                    let mut received_bytes = Vec::new();

                    while let Ok(msg) = rx.try_recv() {
                        match msg {
                            ReaderMessage::Data(chunk) => {
                                received_bytes.extend(chunk);
                            }
                            ReaderMessage::Eof => {
                                exited = true;
                                break;
                            }
                            ReaderMessage::Error(error) => {
                                read_error = Some(error);
                                break;
                            }
                        }
                    }

                    if !received_bytes.is_empty() {
                        let _ = this.update(cx, |panel, cx| {
                            for byte in received_bytes {
                                panel.parser.advance(&mut panel.state, &[byte]);
                            }
                            for response in panel.state.take_responses() {
                                panel.write_input(&response);
                            }
                            panel.scroll_handle.scroll_to_bottom();
                            cx.notify();
                        });
                    }

                    if exited || read_error.is_some() {
                        let _ = this.update(cx, |panel, cx| {
                            panel.process_exited = true;
                            if let Some(error) = read_error {
                                panel.spawn_error =
                                    Some(format!("Terminal stream failed: {error}"));
                            }
                            cx.notify();
                        });
                        break;
                    }

                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(16))
                        .await;
                });

                (Some(writer), Some(master), Some(child), task, shell, None)
            }
            Err(error) => {
                let task = Task::ready(());
                (None, None, None, task, "Terminal".to_string(), Some(error))
            }
        };

        let process_exited = spawn_error.is_some();
        Self {
            focus_handle: cx.focus_handle(),
            state,
            parser,
            writer,
            master,
            child,
            _reader_task: reader_task,
            scroll_handle: ScrollHandle::new(),
            default_fg,
            default_bg,
            shell,
            cwd_label,
            spawn_error,
            process_exited,
        }
    }

    fn spawn_process(
        launch: &TerminalLaunch,
        cols: usize,
        rows: usize,
    ) -> Result<(TerminalProcess, String), String> {
        if let TerminalLaunch::Local { cwd } = launch {
            if !cwd.is_dir() {
                return Err(format!("Project folder is unavailable: {}", cwd.display()));
            }
        }
        let (mut command, shell) = launch.command()?;
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let pty_system = native_pty_system();
        let pty_pair = pty_system
            .openpty(PtySize {
                rows: rows.min(u16::MAX as usize) as u16,
                cols: cols.min(u16::MAX as usize) as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Unable to create a terminal: {error}"))?;

        let child = pty_pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Unable to start {shell}: {error}"))?;
        let reader = pty_pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Unable to read terminal output: {error}"))?;
        let writer = pty_pair
            .master
            .take_writer()
            .map_err(|error| format!("Unable to send terminal input: {error}"))?;
        Ok(((reader, writer, pty_pair.master, child), shell))
    }

    fn write_input(&mut self, data: &[u8]) {
        if let Some(ref mut writer) = self.writer {
            if let Err(error) = writer.write_all(data).and_then(|_| writer.flush()) {
                self.spawn_error = Some(format!("Unable to send terminal input: {error}"));
            }
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let text = text.replace("\r\n", "\n").replace('\n', "\r");
        self.write_input(text.as_bytes());
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if modifiers.control && key == "`" {
            return;
        }
        if (modifiers.control && modifiers.shift && key == "v")
            || (modifiers.platform && key == "v")
        {
            self.paste(cx);
            cx.stop_propagation();
            return;
        }
        if let Some(bytes) = terminal_input_bytes(&event.keystroke) {
            self.write_input(&bytes);
        }
        cx.stop_propagation();
    }

    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    pub fn is_running(&self) -> bool {
        self.spawn_error.is_none() && !self.process_exited
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if self.state.cols == cols && self.state.rows_count == rows {
            return;
        }
        if let Some(master) = self.master.as_ref() {
            if let Err(error) = master.resize(PtySize {
                rows: rows.min(u16::MAX as usize) as u16,
                cols: cols.min(u16::MAX as usize) as u16,
                pixel_width: 0,
                pixel_height: 0,
            }) {
                self.spawn_error = Some(format!("Unable to resize terminal: {error}"));
                return;
            }
        }
        self.state.resize(cols, rows);
    }
}

fn terminal_input_bytes(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let key = keystroke.key.as_str();
    let modifiers = keystroke.modifiers;
    let sequence = match key {
        "enter" => Some("\r"),
        "space" => Some(" "),
        "backspace" => Some("\x7f"),
        "tab" => Some("\t"),
        "escape" => Some("\x1b"),
        "up" => Some("\x1b[A"),
        "down" => Some("\x1b[B"),
        "right" => Some("\x1b[C"),
        "left" => Some("\x1b[D"),
        "home" => Some("\x1b[H"),
        "end" => Some("\x1b[F"),
        "delete" => Some("\x1b[3~"),
        "insert" => Some("\x1b[2~"),
        "pageup" => Some("\x1b[5~"),
        "pagedown" => Some("\x1b[6~"),
        _ => None,
    };
    if let Some(sequence) = sequence {
        return Some(sequence.as_bytes().to_vec());
    }

    let text = keystroke.key_char.as_deref().unwrap_or(key);
    if modifiers.control {
        let byte = match text.as_bytes() {
            [byte] if byte.is_ascii_alphabetic() => byte.to_ascii_lowercase() - b'a' + 1,
            b" " | b"@" => 0,
            b"[" => 27,
            b"\\" => 28,
            b"]" => 29,
            b"^" => 30,
            b"_" => 31,
            _ => return None,
        };
        return Some(vec![byte]);
    }

    if modifiers.platform || modifiers.function {
        return None;
    }

    let mut bytes = Vec::with_capacity(text.len() + usize::from(modifiers.alt));
    if modifiers.alt {
        bytes.push(0x1b);
    }
    bytes.extend_from_slice(text.as_bytes());
    (!bytes.is_empty()).then_some(bytes)
}

impl Render for TerminalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cursor_row = self.state.cursor_render_row();
        let cursor_col = self.state.cursor_col.min(self.state.cols.saturating_sub(1));
        let focused = self.focus_handle.is_focused(window);
        let rows = self
            .state
            .rendered_rows()
            .map(|(index, row)| (index, row.clone()))
            .collect::<Vec<_>>();
        let focus_handle = self.focus_handle.clone();

        div()
            .id("terminal-panel")
            .key_context("DevHubTerminal")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(self.default_bg)
            .font_family(TERMINAL_FONT)
            .text_size(px(11.0))
            .text_color(self.default_fg)
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus_handle.focus(window);
                cx.stop_propagation();
            })
            .on_key_down(cx.listener(Self::handle_key_down))
            .child({
                if let Some(error) = &self.spawn_error {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .px_4()
                        .child(
                            div()
                                .text_color(Hsla {
                                    h: 0.0,
                                    s: 0.7,
                                    l: 0.5,
                                    a: 1.0,
                                })
                                .text_size(px(12.0))
                                .child(format!("Terminal error: {error}")),
                        )
                        .into_any_element()
                } else if self.process_exited {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .px_4()
                        .child(
                            div()
                                .text_color(Hsla {
                                    h: 0.0,
                                    s: 0.0,
                                    l: 0.5,
                                    a: 1.0,
                                })
                                .text_size(px(12.0))
                                .child("Process exited"),
                        )
                        .into_any_element()
                } else {
                    div()
                        .id("terminal-output")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_handle)
                        .child(div().min_h_full().w_full().flex().flex_col().children(
                            rows.into_iter().map(|(index, row)| {
                                let render_len = row
                                    .iter()
                                    .rposition(|cell| cell.text != " ")
                                    .map_or(0, |last| last + 1)
                                    .max(if index == cursor_row {
                                        cursor_col + 1
                                    } else {
                                        0
                                    });
                                div()
                                    .id(("terminal-line", index))
                                    .h(px(16.0))
                                    .w_full()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .px_2()
                                    .font_family(TERMINAL_FONT)
                                    .text_size(px(11.0))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .children(row.into_iter().take(render_len).enumerate().map(
                                        |(column, cell)| {
                                            let cursor = focused
                                                && index == cursor_row
                                                && column == cursor_col;
                                            let mut el = div()
                                                .w(px(7.0))
                                                .h(px(16.0))
                                                .flex_shrink_0()
                                                .font_family(TERMINAL_FONT)
                                                .text_size(px(11.0))
                                                .text_color(if cursor {
                                                    self.default_bg
                                                } else {
                                                    cell.fg
                                                })
                                                .bg(if cursor { self.default_fg } else { cell.bg });
                                            if cell.bold {
                                                el = el.font_weight(FontWeight::BOLD);
                                            }
                                            if cell.italic {
                                                el = el.italic();
                                            }
                                            if cell.underline {
                                                el = el.underline();
                                            }
                                            if cell.strikethrough {
                                                el = el.line_through();
                                            }
                                            el.child(cell.text.clone())
                                        },
                                    ))
                            }),
                        ))
                        .into_any_element()
                }
            })
    }
}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{terminal_input_bytes, TerminalLaunch, TerminalPanel, TerminalState};
    use gpui::{Hsla, Keystroke};
    use std::io::{Read, Write};

    fn test_color(lightness: f32) -> Hsla {
        Hsla {
            h: 0.0,
            s: 0.0,
            l: lightness,
            a: 1.0,
        }
    }

    #[test]
    fn parser_writes_visible_terminal_cells() {
        let mut state = TerminalState::new(12, 3, test_color(0.8), test_color(0.1));
        let mut parser = vte::Parser::new();

        parser.advance(&mut state, b"hello");

        let visible = state.screen[0]
            .iter()
            .map(|cell| cell.text.as_str())
            .collect::<String>();
        assert!(visible.starts_with("hello"));
        assert_eq!(state.cursor_col, 5);
    }

    #[test]
    fn parser_preserves_terminal_colors_and_text_attributes() {
        let default_fg = test_color(0.8);
        let default_bg = test_color(0.1);
        let mut state = TerminalState::new(12, 3, default_fg, default_bg);
        let mut parser = vte::Parser::new();

        parser.advance(
            &mut state,
            b"\x1b[38;2;12;34;56;2;3;4;7;9mA\x1b[0mB\x1b[38:2:78:90:123mC",
        );

        let styled = &state.screen[0][0];
        assert_eq!(
            styled.fg,
            Hsla {
                a: 0.65,
                ..default_bg
            }
        );
        assert_eq!(styled.bg, TerminalState::rgb_color(12, 34, 56));
        assert!(styled.italic);
        assert!(styled.underline);
        assert!(styled.strikethrough);

        let reset = &state.screen[0][1];
        assert_eq!(reset.fg, default_fg);
        assert_eq!(reset.bg, default_bg);

        let colon_truecolor = &state.screen[0][2];
        assert_eq!(colon_truecolor.fg, TerminalState::rgb_color(78, 90, 123));
    }

    #[test]
    fn zero_cursor_coordinates_home_the_terminal() {
        let mut state = TerminalState::new(12, 3, test_color(0.8), test_color(0.1));
        let mut parser = vte::Parser::new();
        state.move_cursor(2, 8);

        parser.advance(&mut state, b"\x1b[0;0H");

        assert_eq!((state.cursor_row, state.cursor_col), (0, 0));
    }

    #[test]
    fn terminal_keys_encode_for_the_pty() {
        let enter = Keystroke::parse("enter").expect("enter should parse");
        let space = Keystroke::parse("space").expect("space should parse");
        let interrupt = Keystroke::parse("ctrl-c").expect("control key should parse");
        let alt = Keystroke::parse("alt-x").expect("alt key should parse");

        assert_eq!(terminal_input_bytes(&enter), Some(b"\r".to_vec()));
        assert_eq!(terminal_input_bytes(&space), Some(b" ".to_vec()));
        assert_eq!(terminal_input_bytes(&interrupt), Some(vec![3]));
        assert_eq!(terminal_input_bytes(&alt), Some(b"\x1bx".to_vec()));
    }

    #[test]
    fn local_pty_accepts_input_and_returns_output() {
        let launch = TerminalLaunch::Local {
            cwd: std::env::current_dir().expect("current directory should be available"),
        };
        let ((mut reader, mut writer, master, mut child), shell) =
            TerminalPanel::spawn_process(&launch, 80, 24)
                .unwrap_or_else(|error| panic!("failed to start {error}"));
        let (output_tx, output_rx) = std::sync::mpsc::sync_channel(256);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        let _ = output_tx.send(Ok(Vec::new()));
                        break;
                    }
                    Ok(read) => {
                        if output_tx.send(Ok(buf[..read].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = output_tx.send(Err(error));
                        break;
                    }
                }
            }
        });

        writer
            .write_all(b"echo DEVHUB_BEFORE_CLEAR\r\nclear\r\necho DEVHUB_PTY_OK\r\nexit\r\n")
            .expect("terminal input should be writable");
        writer.flush().expect("terminal input should flush");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut output = Vec::new();
        let mut state = TerminalState::new(80, 24, test_color(0.8), test_color(0.1));
        let mut parser = vte::Parser::new();
        let status = loop {
            while let Ok(chunk) = output_rx.try_recv() {
                let chunk = chunk.expect("terminal output should be readable");
                output.extend_from_slice(&chunk);
                parser.advance(&mut state, &chunk);
                for response in state.take_responses() {
                    writer
                        .write_all(&response)
                        .expect("terminal response should be writable");
                    writer.flush().expect("terminal response should flush");
                }
            }
            if let Some(status) = child
                .try_wait()
                .unwrap_or_else(|error| panic!("unable to poll {shell}: {error}"))
            {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "{shell} did not process terminal input within 10 seconds: {}",
                    String::from_utf8_lossy(&output)
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        drop(writer);
        drop(master);
        while let Ok(chunk) = output_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            let chunk = chunk.expect("terminal output should be readable");
            if chunk.is_empty() {
                break;
            }
            output.extend_from_slice(&chunk);
            parser.advance(&mut state, &chunk);
        }
        let output = String::from_utf8_lossy(&output);

        assert!(status.success(), "{shell} exited with {status}: {output}");
        assert!(
            output.contains("DEVHUB_PTY_OK"),
            "{shell} did not return the terminal marker: {output}"
        );
        let marker_row = state
            .screen
            .iter()
            .position(|row| {
                row.iter()
                    .map(|cell| cell.text.as_str())
                    .collect::<String>()
                    .contains("DEVHUB_PTY_OK")
            })
            .expect("post-clear marker should remain on screen");
        assert!(marker_row < 8, "clear should home the visible buffer");
    }
}
