use crate::device::CURRENT_DEVICE;
use crate::framebuffer::Display;
use crate::geom::Point;
use crate::input::{
    ABS_MT_TRACKING_ID, DeviceEvent, EV_ABS, EV_KEY, EV_MSC, EV_SYN, FingerStatus, MSC_RAW,
    RawInputEvent, SYN_REPORT, TouchCodes, TouchProto, TouchState, handlers, seconds,
};
use crate::settings::ButtonScheme;
use fxhash::FxHashMap;
use std::mem::{self};
use std::sync::mpsc::{Receiver, Sender};

pub fn handle_touch(
    rx: &Receiver<RawInputEvent>,
    ty: &Sender<DeviceEvent>,
    mut tc: TouchCodes,
    _proto: TouchProto,
    mut button_scheme: ButtonScheme,
    display: Display,
) {
    let mut id = 0;
    let mut last_activity = -60;

    let mut fingers: FxHashMap<i32, Point> = FxHashMap::default();
    let mut buttons: FxHashMap<u16, i32> = FxHashMap::default();
    let mut packets: FxHashMap<i32, TouchState> = FxHashMap::default();

    let Display {
        mut dims,
        mut rotation,
    } = display;

    let (mut mirror_x, mut mirror_y) = CURRENT_DEVICE.should_mirror_axes(rotation);

    packets.insert(id, TouchState::default());

    while let Ok(evt) = rx.recv() {
        match evt.kind {
            EV_ABS => {
                if evt.code == ABS_MT_TRACKING_ID {
                    // Begin of event for a new finger.

                    if evt.value >= 0 {
                        id = evt.value;
                        packets.insert(id, TouchState::default());
                    }
                } else if evt.code == tc.x {
                    if let Some(state) = packets.get_mut(&id) {
                        state.position.x = if mirror_x {
                            dims.0 as i32 - 1 - evt.value
                        } else {
                            evt.value
                        };
                    }
                } else if evt.code == tc.y {
                    if let Some(state) = packets.get_mut(&id) {
                        state.position.y = if mirror_y {
                            dims.1 as i32 - 1 - evt.value
                        } else {
                            evt.value
                        };
                    }
                } else if evt.code == tc.pressure {
                    if let Some(state) = packets.get_mut(&id) {
                        state.pressure = evt.value;

                        if CURRENT_DEVICE.mark() == 3 && state.pressure == 0 {
                            state.position.x = dims.0 as i32 - 1 - state.position.x;
                            mem::swap(&mut state.position.x, &mut state.position.y);
                        }
                    }
                }
            }
            EV_SYN if evt.code == SYN_REPORT => {
                // The absolute value accounts for the wrapping around that might occur,
                // since `tv_sec` can't grow forever.
                if (evt.time.tv_sec - last_activity).abs() >= 60 {
                    last_activity = evt.time.tv_sec;
                    ty.send(DeviceEvent::UserActivity).ok();
                }

                for (&id, state) in &packets {
                    if let Some(_) = fingers.get(&id) {
                        if state.pressure > 0 {
                            ty.send(DeviceEvent::Finger {
                                id,
                                time: seconds(evt.time),
                                status: FingerStatus::Motion,
                                position: state.position,
                            })
                            .unwrap();

                            fingers.insert(id, state.position);
                        } else {
                            ty.send(DeviceEvent::Finger {
                                id,
                                time: seconds(evt.time),
                                status: FingerStatus::Up,
                                position: state.position,
                            })
                            .unwrap();
                            fingers.remove(&id);
                        }
                    } else if state.pressure > 0 {
                        // No fingers were pressing.
                        // If pressure > 0 there is a down event.

                        ty.send(DeviceEvent::Finger {
                            id,
                            time: seconds(evt.time),
                            status: FingerStatus::Down,
                            position: state.position,
                        })
                        .unwrap();
                        fingers.insert(id, state.position);
                    }
                }
            }
            EV_KEY => {
                handlers::handle_key(
                    evt,
                    ty,
                    &mut button_scheme,
                    &mut buttons,
                    &mut tc,
                    &mut rotation,
                    &mut dims,
                    &mut mirror_x,
                    &mut mirror_y,
                );
            }
            EV_MSC if evt.code == MSC_RAW => {
                handlers::handle_msc(evt, ty);
            }
            _ => {}
        }
    }
}
