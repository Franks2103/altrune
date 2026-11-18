//! The app's icons: line drawings on a 24-unit grid, painted with the stroke of the text around
//! them, so they stay sharp at any size and take the colour of either appearance. Nothing is
//! loaded from disk.

use eframe::egui::{epaint::PathShape, pos2, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Bottle,
    Plus,
    Play,
    Download,
    Upload,
    Folder,
    Drive,
    Gear,
    Pulse,
    Search,
    Close,
    ChevronRight,
    Stop,
    Trash,
    Edit,
    Terminal,
    Check,
    Alert,
    Info,
    Gamepad,
    Monitor,
    Film,
    Refresh,
    Layers,
    /// Three dots: more options.
    More,
}

/// Paints `icon` filling `rect` (square) in `color`.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - eframe::egui::vec2(12.0 * s, 12.0 * s);
    let at = |x: f32, y: f32| -> Pos2 { pos2(o.x + x * s, o.y + y * s) };
    let stroke = Stroke::new((1.7 * s).max(1.15), color);
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(
            pts.iter().map(|&(x, y)| at(x, y)).collect(),
            stroke,
        ));
    };
    let closed = |pts: &[(f32, f32)]| {
        p.add(Shape::Path(PathShape::closed_line(
            pts.iter().map(|&(x, y)| at(x, y)).collect(),
            stroke,
        )));
    };
    let circle = |x: f32, y: f32, r: f32| {
        p.circle_stroke(at(x, y), r * s, stroke);
    };
    let dot = |x: f32, y: f32, r: f32| {
        p.circle_filled(at(x, y), r * s, color);
    };
    let rounded = |x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
        p.rect_stroke(
            Rect::from_min_max(at(x0, y0), at(x1, y1)),
            r * s,
            stroke,
            eframe::egui::StrokeKind::Middle,
        );
    };
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| {
        let n = 24;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / n as f32).to_radians();
                at(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        p.add(Shape::line(pts, stroke));
    };
    match icon {
        Icon::Home => {
            line(&[(3.0, 11.0), (12.0, 3.5), (21.0, 11.0)]);
            line(&[(5.5, 9.5), (5.5, 20.0), (18.5, 20.0), (18.5, 9.5)]);
            line(&[(10.0, 20.0), (10.0, 14.5), (14.0, 14.5), (14.0, 20.0)]);
        }
        Icon::Bottle => {
            closed(&[
                (10.0, 3.0),
                (14.0, 3.0),
                (14.0, 7.5),
                (17.5, 11.0),
                (17.5, 21.0),
                (6.5, 21.0),
                (6.5, 11.0),
                (10.0, 7.5),
            ]);
            line(&[(6.5, 14.5), (17.5, 14.5)]);
        }
        Icon::Plus => {
            line(&[(12.0, 5.0), (12.0, 19.0)]);
            line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icon::Play => closed(&[(7.5, 4.5), (19.0, 12.0), (7.5, 19.5)]),
        Icon::Download => {
            line(&[(12.0, 4.0), (12.0, 15.0)]);
            line(&[(7.5, 10.5), (12.0, 15.0), (16.5, 10.5)]);
            line(&[(4.0, 16.5), (4.0, 20.0), (20.0, 20.0), (20.0, 16.5)]);
        }
        Icon::Upload => {
            line(&[(12.0, 15.0), (12.0, 4.0)]);
            line(&[(7.5, 8.5), (12.0, 4.0), (16.5, 8.5)]);
            line(&[(4.0, 16.5), (4.0, 20.0), (20.0, 20.0), (20.0, 16.5)]);
        }
        Icon::Folder => closed(&[
            (3.0, 6.0),
            (9.5, 6.0),
            (11.5, 8.5),
            (21.0, 8.5),
            (21.0, 19.0),
            (3.0, 19.0),
        ]),
        Icon::Drive => {
            rounded(3.0, 13.0, 21.0, 19.5, 2.0);
            line(&[(3.5, 13.0), (6.5, 5.5), (17.5, 5.5), (20.5, 13.0)]);
            dot(16.5, 16.25, 1.1);
        }
        Icon::Gear => {
            circle(12.0, 12.0, 3.0);
            circle(12.0, 12.0, 6.8);
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                line(&[
                    (12.0 + 6.8 * a.cos(), 12.0 + 6.8 * a.sin()),
                    (12.0 + 9.4 * a.cos(), 12.0 + 9.4 * a.sin()),
                ]);
            }
        }
        Icon::Pulse => line(&[
            (3.0, 12.0),
            (7.0, 12.0),
            (10.0, 5.0),
            (14.0, 19.0),
            (17.0, 12.0),
            (21.0, 12.0),
        ]),
        Icon::Search => {
            circle(10.5, 10.5, 6.0);
            line(&[(15.0, 15.0), (20.0, 20.0)]);
        }
        Icon::Close => {
            line(&[(6.5, 6.5), (17.5, 17.5)]);
            line(&[(17.5, 6.5), (6.5, 17.5)]);
        }
        Icon::ChevronRight => line(&[(9.5, 6.0), (15.5, 12.0), (9.5, 18.0)]),
        Icon::Stop => rounded(6.5, 6.5, 17.5, 17.5, 2.0),
        Icon::Trash => {
            line(&[(4.0, 7.0), (20.0, 7.0)]);
            line(&[(9.0, 7.0), (9.0, 4.0), (15.0, 4.0), (15.0, 7.0)]);
            line(&[(6.0, 7.0), (7.0, 20.0), (17.0, 20.0), (18.0, 7.0)]);
        }
        Icon::Edit => closed(&[
            (4.0, 20.0),
            (4.5, 16.0),
            (15.5, 5.0),
            (19.0, 8.5),
            (8.0, 19.5),
        ]),
        Icon::Terminal => {
            line(&[(4.5, 7.0), (9.5, 12.0), (4.5, 17.0)]);
            line(&[(12.0, 17.0), (19.5, 17.0)]);
        }
        Icon::Check => line(&[(5.0, 12.5), (10.0, 17.0), (19.0, 7.0)]),
        Icon::Alert => {
            closed(&[(12.0, 3.5), (21.5, 20.0), (2.5, 20.0)]);
            line(&[(12.0, 9.5), (12.0, 14.0)]);
            dot(12.0, 17.0, 1.15);
        }
        Icon::Info => {
            circle(12.0, 12.0, 9.0);
            line(&[(12.0, 11.0), (12.0, 16.5)]);
            dot(12.0, 7.8, 1.15);
        }
        Icon::Gamepad => {
            rounded(2.5, 7.0, 21.5, 18.0, 5.0);
            line(&[(6.0, 12.5), (10.0, 12.5)]);
            line(&[(8.0, 10.5), (8.0, 14.5)]);
            dot(15.0, 11.5, 1.2);
            dot(17.5, 13.75, 1.2);
        }
        Icon::Monitor => {
            rounded(3.0, 4.0, 21.0, 16.0, 2.0);
            line(&[(12.0, 16.0), (12.0, 20.0)]);
            line(&[(8.0, 20.0), (16.0, 20.0)]);
        }
        Icon::Film => {
            rounded(4.0, 4.0, 20.0, 20.0, 2.0);
            line(&[(8.5, 4.0), (8.5, 20.0)]);
            line(&[(15.5, 4.0), (15.5, 20.0)]);
            line(&[(4.0, 12.0), (20.0, 12.0)]);
        }
        Icon::Refresh => {
            arc(12.0, 12.0, 7.5, -60.0, 230.0);
            line(&[(15.75, 2.5), (15.75, 5.5), (18.8, 5.5)]);
        }
        Icon::More => {
            dot(5.5, 12.0, 1.7);
            dot(12.0, 12.0, 1.7);
            dot(18.5, 12.0, 1.7);
        }
        Icon::Layers => {
            closed(&[(12.0, 3.5), (21.0, 8.0), (12.0, 12.5), (3.0, 8.0)]);
            line(&[(3.0, 12.0), (12.0, 16.5), (21.0, 12.0)]);
            line(&[(3.0, 16.0), (12.0, 20.5), (21.0, 16.0)]);
        }
    }
}
