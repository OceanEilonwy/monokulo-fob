//! Android's stroke icons, as the same SVG path data on the same 24 × 24 grid,
//! drawn with cairo so a button here carries the mark it has there.

use relm4::gtk::{self, cairo, prelude::*};

pub const DOWN: &str = "M12 4v13M6 11l6 6 6-6";
pub const UP: &str = "M12 20V7M6 13l6-6 6 6";
/// A clock with a tick: when it'll all be ready.
pub const CLOCK: &str = "M20.5 13.5A8.5 8.5 0 1 1 12 3.5 M12 7.5V12l2.5 1.5 M16 5.5l2 2 3.5-3.5";
pub const ARROW: &str = "M5 12h13 M13 6l6 6-6 6";
pub const PLUS: &str = "M12 5v14M5 12h14";
pub const CHECK: &str = "M5 12.5l4.5 4.5L19 7.5";
pub const SCAN: &str = "M4 8V5a1 1 0 0 1 1-1h3M16 4h3a1 1 0 0 1 1 1v3M20 16v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3M4 12h16";
pub const SHARE: &str = "M12 15V4M7 8l5-5 5 5M5 13v6a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-6";
pub const PICTURE: &str = "M4 5h16v14H4Z M4 16l5-5 4 4 2-2 5 5 M15.5 8.5a1.5 1.5 0 1 0 0.01 0Z";
pub const TRASH: &str = "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13";
pub const LINK: &str = "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1 M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1";
pub const MEET: &str = "M4.5 7.5a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0Z M2.5 20c.5-3.6 2.7-5.5 5.5-5.5s5 1.9 5.5 5.5 M16 9a4 4 0 0 1 0 6 M19.2 6.5a8 8 0 0 1 0 11";
pub const SWAP: &str = "M4 8h15 M15 4l4 4-4 4 M20 16H5 M9 12l-4 4 4 4";
pub const REQUEST: &str = "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h2 M18 14h2 M14 18h2 M18 18h2";
pub const SPLIT: &str = "M2.5 12a4.5 4.5 0 1 0 9 0a4.5 4.5 0 1 0 -9 0Z M16.8 5a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0Z M16.8 12a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0Z M16.8 19a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0Z";

/// `path` on a `side` px square, in the colour of the words beside it, `fade` of its strength.
pub fn icon(path: &'static str, side: i32, fade: f64) -> gtk::DrawingArea {
    stroke(path, side, None, fade)
}

fn stroke(path: &'static str, side: i32, tint: Option<(f64, f64, f64)>, fade: f64) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder().content_width(side).content_height(side).valign(gtk::Align::Center).build();
    area.set_draw_func(move |area, cr, width, height| {
        let scale = f64::from(width.min(height)) / 24.0;
        cr.scale(scale, scale);
        match tint {
            Some((r, g, b)) => cr.set_source_rgb(r, g, b),
            None => {
                let c = area.color();
                cr.set_source_rgba(f64::from(c.red()), f64::from(c.green()), f64::from(c.blue()), f64::from(c.alpha()) * fade);
            }
        }
        cr.set_line_width(2.0);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        trace(cr, path);
        let _ = cr.stroke();
    });
    area
}

/// Adds the SVG path `data` to `cr`'s path: moves, lines, horizontals, verticals, cubics, arcs
/// and closes, absolute and relative.
fn trace(cr: &cairo::Context, data: &str) {
    let mut numbers = Numbers { text: data.as_bytes(), at: 0 };
    let (mut x, mut y) = (0.0_f64, 0.0_f64);
    let (mut start_x, mut start_y) = (0.0_f64, 0.0_f64);
    let mut command = b'M';
    while let Some(next) = numbers.command() {
        if let Some(c) = next {
            command = c;
        }
        let relative = command.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        match command.to_ascii_uppercase() {
            b'M' => {
                let (Some(px), Some(py)) = (numbers.number(), numbers.number()) else { break };
                (x, y) = (ox + px, oy + py);
                (start_x, start_y) = (x, y);
                cr.move_to(x, y);
                // More pairs after a move are lines.
                command = if relative { b'l' } else { b'L' };
            }
            b'L' => {
                let (Some(px), Some(py)) = (numbers.number(), numbers.number()) else { break };
                (x, y) = (ox + px, oy + py);
                cr.line_to(x, y);
            }
            b'H' => {
                let Some(px) = numbers.number() else { break };
                x = ox + px;
                cr.line_to(x, y);
            }
            b'V' => {
                let Some(py) = numbers.number() else { break };
                y = oy + py;
                cr.line_to(x, y);
            }
            b'C' => {
                let mut n = [0.0; 6];
                for slot in &mut n {
                    let Some(v) = numbers.number() else { return };
                    *slot = v;
                }
                cr.curve_to(ox + n[0], oy + n[1], ox + n[2], oy + n[3], ox + n[4], oy + n[5]);
                (x, y) = (ox + n[4], oy + n[5]);
            }
            b'A' => {
                let mut n = [0.0; 5];
                for slot in &mut n {
                    let Some(v) = numbers.number() else { return };
                    *slot = v;
                }
                let (Some(px), Some(py)) = (numbers.number(), numbers.number()) else { break };
                let (ex, ey) = (ox + px, oy + py);
                arc(cr, (x, y), (n[0], n[1]), n[2], n[3] != 0.0, n[4] != 0.0, (ex, ey));
                (x, y) = (ex, ey);
            }
            b'Z' => {
                cr.close_path();
                (x, y) = (start_x, start_y);
            }
            _ => break,
        }
    }
}

/// An SVG elliptical arc from `from` to `to`, by the spec's endpoint-to-centre conversion.
fn arc(cr: &cairo::Context, from: (f64, f64), radii: (f64, f64), rotation: f64, large: bool, sweep: bool, to: (f64, f64)) {
    let (x1, y1) = from;
    let (x2, y2) = to;
    let (mut rx, mut ry) = (radii.0.abs(), radii.1.abs());
    if (x1 == x2 && y1 == y2) || rx == 0.0 || ry == 0.0 {
        cr.line_to(x2, y2);
        return;
    }
    let phi = rotation.to_radians();
    let (cos, sin) = (phi.cos(), phi.sin());
    let (dx, dy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let (x1p, y1p) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let root = lambda.sqrt();
        rx *= root;
        ry *= root;
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut factor = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        factor = -factor;
    }
    let (cxp, cyp) = (factor * rx * y1p / ry, -factor * ry * x1p / rx);
    let (cx, cy) = (cos * cxp - sin * cyp + (x1 + x2) / 2.0, sin * cxp + cos * cyp + (y1 + y2) / 2.0);
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let theta = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let saved = cr.matrix();
    cr.translate(cx, cy);
    cr.rotate(phi);
    cr.scale(rx, ry);
    if sweep {
        cr.arc(0.0, 0.0, 1.0, theta, theta + delta);
    } else {
        cr.arc_negative(0.0, 0.0, 1.0, theta, theta + delta);
    }
    cr.set_matrix(saved);
}

/// Reads path data: a command letter, or numbers (which may run together as `5.7-5.7` or `.5.5`).
struct Numbers<'a> {
    text: &'a [u8],
    at: usize,
}

impl Numbers<'_> {
    fn skip(&mut self) {
        while self.at < self.text.len() && (self.text[self.at].is_ascii_whitespace() || self.text[self.at] == b',') {
            self.at += 1;
        }
    }

    /// The next command: `Some(Some(letter))` for a new one, `Some(None)` for numbers repeating the last, `None` at the end.
    fn command(&mut self) -> Option<Option<u8>> {
        self.skip();
        match self.text.get(self.at) {
            None => None,
            Some(c) if c.is_ascii_alphabetic() => {
                self.at += 1;
                Some(Some(*c))
            }
            Some(_) => Some(None),
        }
    }

    fn number(&mut self) -> Option<f64> {
        self.skip();
        let start = self.at;
        let mut seen_dot = false;
        if matches!(self.text.get(self.at), Some(b'-' | b'+')) {
            self.at += 1;
        }
        while let Some(&c) = self.text.get(self.at) {
            match c {
                b'0'..=b'9' => self.at += 1,
                b'.' if !seen_dot => {
                    seen_dot = true;
                    self.at += 1;
                }
                _ => break,
            }
        }
        std::str::from_utf8(&self.text[start..self.at]).ok()?.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws `path` on a 24 px surface and says which pixels got ink.
    fn ink(path: &str) -> Vec<bool> {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 24, 24).expect("a surface");
        {
            let cr = cairo::Context::new(&surface).expect("a context");
            cr.set_line_width(2.0);
            cr.set_line_cap(cairo::LineCap::Round);
            cr.set_line_join(cairo::LineJoin::Round);
            trace(&cr, path);
            cr.stroke().expect("a stroke");
        }
        drop(surface.clone());
        let mut surface = surface;
        let stride = surface.stride() as usize;
        let data = surface.data().expect("pixels");
        (0..24 * 24).map(|i| data[(i / 24) * stride + (i % 24) * 4 + 3] != 0).collect()
    }

    #[test]
    fn every_icon_draws_inside_its_grid_and_something() {
        for (name, path) in [("down", DOWN), ("up", UP), ("arrow", ARROW), ("plus", PLUS), ("check", CHECK), ("scan", SCAN), ("share", SHARE), ("picture", PICTURE), ("trash", TRASH), ("link", LINK), ("meet", MEET), ("swap", SWAP), ("request", REQUEST), ("split", SPLIT)] {
            let pixels = ink(path);
            assert!(pixels.iter().filter(|p| **p).count() > 20, "{name} drew nothing");
        }
    }

    #[test]
    fn arcs_land_where_the_path_says() {
        // The picture's sun: a tiny circle at (15.5, 8.5) with a 1.5 radius.
        let picture = ink(PICTURE);
        assert!(picture[8 * 24 + 15], "the sun is missing");
        // The split's big coin: a circle of radius 4.5 round (7, 12): its left edge, not its middle.
        let split = ink(SPLIT);
        assert!(split[12 * 24 + 2], "the coin's left edge is missing");
        assert!(!split[12 * 24 + 7], "the coin is filled");
        // Two arrows' heads in Swap, at (19, 8) and (5, 16).
        let swap = ink(SWAP);
        assert!(swap[8 * 24 + 19] && swap[16 * 24 + 5]);
    }
}
