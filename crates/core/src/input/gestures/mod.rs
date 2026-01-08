use crate::device::CURRENT_DEVICE;
use crate::geom::{elbow, nearest_segment_point, Axis, DiagDir, Dir, Point, Vec2};
use crate::input::gestures::state_machine::StateMachine;
use crate::input::{ButtonCode, DeviceEvent};
use crate::unit::mm_to_px;
use crate::view::Event;
use std::f64;
use std::fmt;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

mod state_machine;

pub const TAP_JITTER_MM: f32 = 4.;
pub const HOLD_JITTER_MM: f32 = 2.;
pub const HOLD_DELAY_SHORT_SEC: f64 = 0.666;
pub const HOLD_DELAY_LONG_SEC: f64 = 1.333;

#[derive(Debug, Clone)]
pub enum GestureEvent {
    Tap(Point),
    MultiTap([Point; 2]),
    Movement {
        start: Point,
        end: Point,
    },
    MovementEnded {},
    Swipe {
        dir: Dir,
        start: Point,
        end: Point,
    },
    SlantedSwipe {
        dir: DiagDir,
        start: Point,
        end: Point,
    },
    MultiSwipe {
        dir: Dir,
        starts: [Point; 2],
        ends: [Point; 2],
    },
    Arrow {
        dir: Dir,
        start: Point,
        end: Point,
    },
    MultiArrow {
        dir: Dir,
        starts: [Point; 2],
        ends: [Point; 2],
    },
    Corner {
        dir: DiagDir,
        start: Point,
        end: Point,
    },
    MultiCorner {
        dir: DiagDir,
        starts: [Point; 2],
        ends: [Point; 2],
    },
    Pinch {
        axis: Axis,
        center: Point,
        factor: f32,
    },
    Spread {
        axis: Axis,
        center: Point,
        factor: f32,
    },
    Rotate {
        center: Point,
        quarter_turns: i8,
        angle: f32,
    },
    Cross(Point),
    Diamond(Point),
    HoldFingerShort(Point, i32),
    HoldFingerLong(Point, i32),
    HoldButtonShort(ButtonCode),
    HoldButtonLong(ButtonCode),
}

impl fmt::Display for GestureEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GestureEvent::Tap(pt) => write!(f, "Tap {}", pt),
            GestureEvent::MultiTap(pts) => write!(f, "Multitap {} {}", pts[0], pts[1]),
            GestureEvent::Movement { start, end } => write!(f, "Movement {start} -> {end}"),
            GestureEvent::MovementEnded {} => write!(f, "Movement ended"),
            GestureEvent::Swipe { dir, .. } => write!(f, "Swipe {}", dir),
            GestureEvent::SlantedSwipe { dir, .. } => write!(f, "SlantedSwipe {}", dir),
            GestureEvent::MultiSwipe { dir, .. } => write!(f, "Multiswipe {}", dir),
            GestureEvent::Arrow { dir, .. } => write!(f, "Arrow {}", dir),
            GestureEvent::MultiArrow { dir, .. } => write!(f, "Multiarrow {}", dir),
            GestureEvent::Corner { dir, .. } => write!(f, "Corner {}", dir),
            GestureEvent::MultiCorner { dir, .. } => write!(f, "Multicorner {}", dir),
            GestureEvent::Pinch {
                axis,
                center,
                factor,
                ..
            } => write!(f, "Pinch {} {} {:.2}", axis, center, factor),
            GestureEvent::Spread {
                axis,
                center,
                factor,
                ..
            } => write!(f, "Spread {} {} {:.2}", axis, center, factor),
            GestureEvent::Rotate {
                center,
                quarter_turns,
                ..
            } => write!(f, "Rotate {} {}", center, *quarter_turns as i32 * 90),
            GestureEvent::Cross(pt) => write!(f, "Cross {}", pt),
            GestureEvent::Diamond(pt) => write!(f, "Diamond {}", pt),
            GestureEvent::HoldFingerShort(pt, id) => write!(f, "Short-held finger {} {}", id, pt),
            GestureEvent::HoldFingerLong(pt, id) => write!(f, "Long-held finger {} {}", id, pt),
            GestureEvent::HoldButtonShort(code) => write!(f, "Short-held button {:?}", code),
            GestureEvent::HoldButtonLong(code) => write!(f, "Long-held button {:?}", code),
        }
    }
}

fn interpret_segment(sp: &[Point]) -> GestureEvent {
    let a = sp[0];
    let b = sp[sp.len() - 1];
    let ab = b - a;
    let d = ab.length();

    let p = sp[elbow(sp)];
    let (n, p) = {
        let p: Vec2 = p.into();
        let (n, _) = nearest_segment_point(p, a.into(), b.into());
        (n, p)
    };
    let np = p - n;
    let ds = np.length();
    if ds > d / 5.0 {
        let g = (np.x as f32 / np.y as f32).abs();
        if g < 0.5 || g > 2.0 {
            GestureEvent::Arrow {
                dir: np.dir(),
                start: a,
                end: b,
            }
        } else {
            GestureEvent::Corner {
                dir: np.diag_dir(),
                start: a,
                end: b,
            }
        }
    } else {
        let g = (ab.x as f32 / ab.y as f32).abs();
        if g < 0.5 || g > 2.0 {
            GestureEvent::Swipe {
                start: a,
                end: b,
                dir: ab.dir(),
            }
        } else {
            GestureEvent::SlantedSwipe {
                start: a,
                end: b,
                dir: ab.diag_dir(),
            }
        }
    }
}

fn interpret_double_gesture(f1_pos: &[Point], f2_pos: &[Point]) -> Option<GestureEvent> {
    let g1 = interpret_segment(f1_pos);
    let g2 = interpret_segment(f2_pos);

    if let GestureEvent::Swipe {
        dir: d1,
        start: s1,
        end: e1,
    } = g1
    {
        if let GestureEvent::Swipe {
            dir: d2,
            start: s2,
            end: e2,
        } = g2
        {
            if d2 == d1.opposite() {
                let center = (s1 + s2) / 2;
                let ds = (s2 - s1).length();
                let de = (e2 - e1).length();
                let factor = de / ds;
                return Some(if factor < 1.0 {
                    GestureEvent::Pinch {
                        axis: d1.axis(),
                        center,
                        factor,
                    }
                } else {
                    GestureEvent::Spread {
                        axis: d1.axis(),
                        center,
                        factor,
                    }
                });
            }
        }
    }

    if let GestureEvent::SlantedSwipe {
        dir: d1,
        start: s1,
        end: e1,
    } = g1
    {
        if let GestureEvent::SlantedSwipe {
            dir: d2,
            start: s2,
            end: e2,
        } = g2
        {
            if d2 == d1.opposite() {
                let center = (s1 + s2) / 2;
                let ds = (s2 - s1).length();
                let de = (e2 - e1).length();
                let factor = de / ds;
                return Some(if factor < 1.0 {
                    GestureEvent::Pinch {
                        axis: Axis::Diagonal,
                        center,
                        factor,
                    }
                } else {
                    GestureEvent::Spread {
                        axis: Axis::Diagonal,
                        center,
                        factor,
                    }
                });
            }
        }
    }

    None
}

pub fn abs_variance(segs: &[Point]) -> i32 {
    let len = segs.len() as i32;

    // Pick only some points
    let mut avg = segs.iter().fold(0, |mut acc, p| {
        acc += p.length() as i32;
        acc
    });
    avg = avg / len;

    let var = segs.iter().fold(0, |mut acc, p| {
        acc += (p.length() as i32 - avg).pow(2);
        acc
    });
    var / len
}

pub fn x_y_variance(segs: &[Point]) -> (i32, i32) {
    let len = segs.len().div_ceil(2) as i32;

    // Pick only some points
    let (mut avg_x, mut avg_y) =
        segs.iter()
            .step_by(2)
            .fold((0, 0), |(mut acc_x, mut acc_y), p| {
                acc_x += p.x;
                acc_y += p.y;
                (acc_x, acc_y)
            });
    (avg_x, avg_y) = (avg_x / len, avg_y / len);

    let (mut var_x, mut var_y) =
        segs.iter()
            .step_by(2)
            .fold((0, 0), |(mut acc_x, mut acc_y), p| {
                acc_x += (p.x - avg_x).pow(2);
                acc_y += (p.y - avg_y).pow(2);
                (acc_x, acc_y)
            });
    (var_x, var_y) = (var_x / len, var_y / len);

    (var_x, var_y)
}

fn is_within_jitter(points: &[Point], jitter_mm: f32) -> bool {
    let len = points.len().div_ceil(2) as i32;
    let jitter_px = mm_to_px(jitter_mm, CURRENT_DEVICE.dpi);

    let (mut avg_x, mut avg_y) =
        points
            .iter()
            .step_by(2)
            .fold((0, 0), |(mut acc_x, mut acc_y), p| {
                acc_x += p.x;
                acc_y += p.y;
                (acc_x, acc_y)
            });
    (avg_x, avg_y) = (avg_x / len, avg_y / len);

    (avg_x - points[0].x).abs() <= jitter_px && (avg_y - points[0].y).abs() <= jitter_px
}

pub fn gesture_events(device_ev_rx: Receiver<DeviceEvent>, kaesar_ev_tx: Sender<Event>) {
    let mut sm = StateMachine::new();

    thread::spawn(move || {
        while let Ok(evt) = device_ev_rx.recv() {
            kaesar_ev_tx.send(Event::Device(evt)).ok();

            sm = sm.transition(evt, &kaesar_ev_tx);
        }
    });
}
