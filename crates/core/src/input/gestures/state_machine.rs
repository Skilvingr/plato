use crate::geom::Point;
use crate::input::gestures::{
    GestureEvent, HOLD_DELAY_LONG_SEC, HOLD_DELAY_SHORT_SEC, HOLD_JITTER_MM, TAP_JITTER_MM,
    abs_variance, interpret_double_gesture, interpret_segment, is_within_jitter, x_y_variance,
};
use crate::input::{ButtonStatus, DeviceEvent, FingerStatus};
use crate::view::Event;
use std::f64;
use std::sync::mpsc::Sender;

/// State of a finger.
#[derive(Debug)]
pub struct FingerState {
    /// The id of the finger.
    id: i32,
    /// The timestamp of the state.
    time: f64,
    /// Whether the finger is pressed.
    is_down: bool,
    /// Whether the finger is being held.
    held: bool,
    /// Whether the finger is being long held.
    long_held: bool,
    /// Whether a movement has been started => No hold is possible anymore.
    movement_started: bool,
    /// The positions covered by the finger in its life.
    positions: Vec<Point>,
}

/// State of a button.
struct BtnState {
    /// The timestamp related to the `down` event.
    time: f64,
    /// Whether the button is being held.
    held: bool,
    /// Whether the button is being long held.
    long_held: bool,
}

/// Possible states for touch screen.
enum TouchStates {
    NoTouch,
    OneFinger([FingerState; 1]),
    TwoFingers([FingerState; 2]),
}

/// Possible states for a button (on button at a time).
enum BtnStates {
    NotPressed,
    Pressed(BtnState),
}

impl TouchStates {
    /// Transition from a state to the next: the finger `f_state` has just been pressed.
    pub fn next(self, f_state: FingerState) -> TouchStates {
        match self {
            TouchStates::NoTouch => Self::OneFinger([f_state]),
            TouchStates::OneFinger([f1]) => Self::TwoFingers([f1, f_state]),
            TouchStates::TwoFingers(_) => unreachable!(),
        }
    }
}

/// State machine to interpret input device events.
pub struct StateMachine {
    state: TouchStates,
    btn_state: BtnStates,
}

impl StateMachine {
    /// Create a new state machine.
    pub fn new() -> Self {
        let arr: Vec<Point> = (220..1024).map(|i| Point::new(1024, i)).collect();
        let (v_x, v_y) = x_y_variance(&arr);
        println!("{} | {} - {}", abs_variance(&arr), v_x, v_y);

        Self {
            state: TouchStates::NoTouch,
            btn_state: BtnStates::NotPressed,
        }
    }

    /// Handles a `finger down` event.
    #[inline]
    fn handle_finger_down(
        fingers: &mut [FingerState],
        id: i32,
        time: f64,
        position: Point,
    ) -> Option<FingerState> {
        // No other finger must have the same id or be held.

        if !fingers.iter().any(|f| f.held || f.id == id) {
            // A new finger has just joined; reset the state of the other fingers.
            fingers.iter_mut().for_each(|f| f.positions.truncate(1));

            Some(FingerState {
                id,
                time,
                is_down: true,
                held: false,
                long_held: false,
                movement_started: false,
                positions: vec![position],
            })
        } else {
            None
        }
    }

    /// Handles a `finger moved` event.
    #[inline]
    fn handle_finger_motion(
        ty: &Sender<Event>,
        fingers: &mut [FingerState],
        id: i32,
        time: f64,
        position: Point,
    ) {
        if let Some(f) = fingers.iter_mut().find(|e| e.id == id) {
            f.positions.push(position);

            let can_hold = !f.movement_started && is_within_jitter(&f.positions, HOLD_JITTER_MM);

            if can_hold {
                let delta_t = time - f.time;
                if delta_t >= HOLD_DELAY_SHORT_SEC {
                    //let var = x_y_variance(&f.positions);

                    if delta_t >= HOLD_DELAY_LONG_SEC && !f.long_held {
                        f.long_held = true;
                        let _ = ty.send(Event::Gesture(GestureEvent::HoldFingerLong(position, id)));
                    } else if !f.held {
                        f.held = true;
                        let _ =
                            ty.send(Event::Gesture(GestureEvent::HoldFingerShort(position, id)));
                    }
                }
            } else {
                // If the variance is beyond the hold jitter, it means that a movement has been started
                // an that no hold events should be triggered anymore in this state.
                f.movement_started = true;

                let len = f.positions.len();
                if len > 30 {
                    let a = f.positions[0];
                    let b = f.positions[len - 1];

                    let _ = ty.send(Event::Gesture(GestureEvent::Movement { start: a, end: b }));

                    f.positions.drain(..len - 10);
                    //handle_swipe
                }
            }
        }
    }

    /// Given the device input event `ev`, sends eventual `GestureEvent`s through `kaesar_ev_tx`
    /// and computes the next state of the state machine.
    pub fn transition(mut self, ev: DeviceEvent, kaesar_ev_tx: &Sender<Event>) -> Self {
        match ev {
            DeviceEvent::Finger {
                id,
                time,
                status,
                position,
            } => match self.state {
                TouchStates::NoTouch => {
                    self.state = TouchStates::OneFinger([FingerState {
                        id,
                        time,
                        is_down: true,
                        held: false,
                        long_held: false,
                        movement_started: false,
                        positions: vec![position],
                    }]);
                }
                TouchStates::OneFinger(ref mut fingers) => match status {
                    FingerStatus::Down => {
                        if let Some(ts) = Self::handle_finger_down(fingers, id, time, position) {
                            self.state = self.state.next(ts);
                        }
                    }
                    FingerStatus::Motion => {
                        Self::handle_finger_motion(&kaesar_ev_tx, fingers, id, time, position);

                        if !fingers[0].is_down {
                            self.state = TouchStates::NoTouch;
                        }
                    }
                    FingerStatus::Up => {
                        let finger = &fingers[0];

                        if !finger.held {
                            if !finger.movement_started
                                && is_within_jitter(&finger.positions, TAP_JITTER_MM)
                            {
                                let _ = kaesar_ev_tx
                                    .send(Event::Gesture(GestureEvent::Tap(finger.positions[0])));
                            } else {
                                // let (v_x, v_y) = x_y_variance(&finger.positions);
                                // println!(
                                //     "{} | {} | {} - {}",
                                //     finger.positions.len(),
                                //     abs_variance(&finger.positions),
                                //     v_x,
                                //     v_y
                                // );

                                let _ = kaesar_ev_tx
                                    .send(Event::Gesture(GestureEvent::MovementEnded {}));
                                let g = interpret_segment(&finger.positions);
                                //println!("Gesture: {}", g.to_string());
                                let _ = kaesar_ev_tx.send(Event::Gesture(g));
                            }
                        }

                        self.state = TouchStates::NoTouch;
                    }
                },
                TouchStates::TwoFingers(ref mut fingers) => {
                    match status {
                        FingerStatus::Down => {}
                        FingerStatus::Motion => {
                            Self::handle_finger_motion(&kaesar_ev_tx, fingers, id, time, position);

                            // if let Some((i, _)) = fingers.iter().enumerate().find(|(_, f)| !f.is_down) {
                            //     self.state = self.state.prev(i);
                            // }
                        }
                        FingerStatus::Up => {
                            let mut all_up = true;
                            let mut small_vars = true;

                            for finger in fingers.iter_mut() {
                                if finger.id == id {
                                    finger.is_down = false;
                                }

                                all_up = all_up && !finger.is_down && !finger.held;

                                if !finger.is_down {
                                    small_vars = small_vars
                                        && is_within_jitter(&finger.positions, TAP_JITTER_MM)
                                }
                            }

                            if all_up {
                                let [f1, f2] = fingers;

                                if small_vars {
                                    let _ = kaesar_ev_tx.send(Event::Gesture(
                                        GestureEvent::MultiTap([f1.positions[0], f2.positions[0]]),
                                    ));
                                } else {
                                    if let Some(g) =
                                        interpret_double_gesture(&f1.positions, &f2.positions)
                                    {
                                        let _ = kaesar_ev_tx.send(Event::Gesture(g));
                                    }
                                }

                                self.state = TouchStates::NoTouch;
                            }
                        }
                    }
                }
            },
            DeviceEvent::Button {
                status: ButtonStatus::Pressed,
                time: ev_time,
                code,
            } => match self.btn_state {
                BtnStates::NotPressed => {
                    self.btn_state = BtnStates::Pressed(BtnState {
                        time: ev_time,
                        held: false,
                        long_held: false,
                    })
                }
                BtnStates::Pressed(BtnState {
                    time,
                    ref mut held,
                    ref mut long_held,
                }) => {
                    let delta_t = ev_time - time;

                    if delta_t >= HOLD_DELAY_SHORT_SEC {
                        if !*long_held && delta_t >= HOLD_DELAY_LONG_SEC {
                            *long_held = true;
                            kaesar_ev_tx
                                .send(Event::Gesture(GestureEvent::HoldButtonLong(code)))
                                .ok();
                        } else if !*held {
                            *held = true;
                            kaesar_ev_tx
                                .send(Event::Gesture(GestureEvent::HoldButtonShort(code)))
                                .ok();
                        }
                    }
                }
            },
            DeviceEvent::Button {
                status: ButtonStatus::Released,
                ..
            } => {
                self.btn_state = BtnStates::NotPressed;
            }
            _ => {}
        }

        self
    }
}
