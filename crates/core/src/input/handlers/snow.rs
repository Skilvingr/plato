use crate::device::CURRENT_DEVICE;
use crate::framebuffer::Display;
use crate::geom::Point;
use crate::input::{
    ABS_MT_TRACKING_ID, ButtonCode, ButtonStatus, DeviceEvent, EV_ABS, EV_KEY, EV_MSC, EV_SYN,
    FingerStatus, MSC_RAW, RawInputEvent, SYN_REPORT, TouchCodes, TouchProto, TouchState, handlers,
    seconds,
};
use crate::settings::ButtonScheme;
use fxhash::FxHashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

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

    let mut last_finger_timestamp = 0.;
    let mut last_btn_timestamp = 0.;
    loop {
        // No fingers nor buttons down; wait indefinitely for new packets.
        if let Ok(evt) = if fingers.is_empty() && buttons.is_empty() {
            rx.recv().map_err(Into::into)
        } else {
            // There's at least a finger or a button down; wait for 10 ms and use the last known finger
            // if nothing comes down the channel.
            let res = rx.recv_timeout(Duration::from_millis(20));

            // Nothing came down the channell; repeat the last events.
            if res.is_err() {
                if !fingers.is_empty() {
                    last_finger_timestamp += 0.02;
                }
                if !buttons.is_empty() {
                    last_btn_timestamp += 0.02;
                }

                for f in fingers.iter() {
                    let _ = ty.send(DeviceEvent::Finger {
                        id: *f.0,
                        time: last_finger_timestamp,
                        status: FingerStatus::Motion,
                        position: *f.1,
                    });
                }

                for b in buttons.iter() {
                    if let Some(button_status) = ButtonStatus::try_from_raw(*b.1) {
                        let _ = ty.send(DeviceEvent::Button {
                            time: last_btn_timestamp,
                            code: ButtonCode::from_raw(*b.0, rotation, button_scheme),
                            status: button_status,
                        });
                    }
                }
            }

            res
        } {
            match evt.kind {
                EV_ABS => {
                    if evt.code == ABS_MT_TRACKING_ID {
                        // Begin of event for a new finger.
                        if evt.value >= 0 {
                            id = evt.value;
                            packets.insert(id, TouchState::default());
                            last_finger_timestamp = seconds(evt.time);
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
                    }
                }
                EV_SYN if evt.code == SYN_REPORT && evt.value == 0 => {
                    // The absolute value accounts for the wrapping around that might occur,
                    // since `tv_sec` can't grow forever.
                    if (evt.time.tv_sec - last_activity).abs() >= 60 {
                        last_activity = evt.time.tv_sec;
                        ty.send(DeviceEvent::UserActivity).ok();
                    }

                    fingers.retain(|other_id, other_position| {
                        match packets.contains_key(&other_id) {
                            true => true,
                            false => {
                                let _ = ty.send(DeviceEvent::Finger {
                                    id: *other_id,
                                    time: seconds(evt.time),
                                    status: FingerStatus::Up,
                                    position: *other_position,
                                });
                                false
                            }
                        }
                    });

                    for (&id, state) in &packets {
                        if let Some(_) = fingers.get(&id) {
                            // The finger is already known; move it.
                            ty.send(DeviceEvent::Finger {
                                id,
                                time: seconds(evt.time),
                                status: FingerStatus::Motion,
                                position: state.position,
                            })
                            .unwrap();

                            fingers.insert(id, state.position);
                        } else {
                            // New finger; send Down event.
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

                    packets.clear();
                }
                EV_KEY => {
                    last_btn_timestamp = seconds(evt.time);
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
}
