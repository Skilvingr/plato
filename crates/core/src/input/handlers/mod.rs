use std::{
    mem,
    sync::mpsc::{Receiver, Sender},
};

use fxhash::FxHashMap;

use crate::{
    device::CURRENT_DEVICE,
    framebuffer::Display,
    input::{
        ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_PRESSURE, ABS_MT_SLOT, ABS_MT_TRACKING_ID,
        ABS_PRESSURE, BTN_TOUCH, BTN_TOUCH_2, ButtonCode, ButtonStatus, DeviceEvent, EV_ABS,
        EV_KEY, EV_SYN, GYROSCOPE_ROTATIONS, KEY_BUTTON_SCHEME, KEY_POWER, KEY_ROTATE_DISPLAY,
        MSC_RAW_GSENSOR_LANDSCAPE_LEFT, MSC_RAW_GSENSOR_PORTRAIT_DOWN, RawInputEvent, SLEEP_COVER,
        SYN_MT_REPORT, SYN_REPORT, TouchCodes, TouchProto, VAL_PRESS, VAL_RELEASE, VAL_REPEAT,
        seconds,
    },
    settings::ButtonScheme,
};

pub mod multi_touch;
pub mod single_touch;
pub mod snow;

pub type InputHandler = fn(
    &Receiver<RawInputEvent>,
    &Sender<DeviceEvent>,
    TouchCodes,
    TouchProto,
    ButtonScheme,
    Display,
);

pub fn debug_event(event: &RawInputEvent) {
    let t = match event.kind {
        EV_ABS => "EV_ABS",
        EV_KEY => "EV_KEY",
        EV_SYN => "EV_SYN",
        _ => "UNKNOWN",
    };

    let c = match event.code {
        ABS_MT_SLOT => "ABS_MT_SLOT",
        ABS_MT_TRACKING_ID => "ABS_MT_TRACKING_ID",
        ABS_PRESSURE => "ABS_PRESSURE",
        ABS_MT_PRESSURE => "ABS_MT_PRESSURE",
        ABS_MT_POSITION_X => "ABS_MT_POSITION_X",
        ABS_MT_POSITION_Y => "ABS_MT_POSITION_Y",
        SYN_REPORT => "SYN_REPORT",
        SYN_MT_REPORT => "SYN_MT_REPORT",
        KEY_POWER => "KEY_POWER",
        key if event.kind == EV_KEY => &format!("BTN_CODE: {key}"),
        _ => "UNKNOWN",
    };

    if t != "UNKNOWN" && c != "UNKNOWN" {
        println!("{} | {t} {c} {}", seconds(event.time), event.value);
    }
}

pub fn handle_key(
    evt: RawInputEvent,
    ty: &Sender<DeviceEvent>,
    button_scheme: &mut ButtonScheme,
    buttons: &mut FxHashMap<u16, i32>,
    _tc: &mut TouchCodes,
    rotation: &mut i8,
    dims: &mut (u32, u32),
    _mirror_x: &mut bool,
    _mirror_y: &mut bool,
) {
    if evt.value == VAL_RELEASE {
        let _ = buttons.remove(&evt.code);
    } else {
        buttons.insert(evt.code, evt.value);
    }

    if SLEEP_COVER.contains(&evt.code) {
        if evt.value == VAL_PRESS {
            ty.send(DeviceEvent::CoverOn).ok();
        } else if evt.value == VAL_RELEASE {
            ty.send(DeviceEvent::CoverOff).ok();
        } else if evt.value == VAL_REPEAT {
            ty.send(DeviceEvent::CoverOn).ok();
        }
    } else if evt.code == KEY_BUTTON_SCHEME {
        if evt.value == VAL_PRESS {
            *button_scheme = ButtonScheme::Inverted;
        } else {
            *button_scheme = ButtonScheme::Natural;
        }
    } else if evt.code == KEY_ROTATE_DISPLAY {
        let next_rotation = evt.value as i8;
        if next_rotation != *rotation {
            let delta = (*rotation - next_rotation).abs();
            if delta % 2 == 1 {
                #[cfg(not(feature = "sim"))]
                mem::swap(&mut _tc.x, &mut _tc.y);

                mem::swap(&mut dims.0, &mut dims.1);
            }
            *rotation = next_rotation;

            #[cfg(not(feature = "sim"))]
            {
                let should_mirror = CURRENT_DEVICE.should_mirror_axes(*rotation);
                *_mirror_x = should_mirror.0;
                *_mirror_y = should_mirror.1;
            }
        }
    } else if evt.code != BTN_TOUCH && evt.code != BTN_TOUCH_2 {
        if let Some(button_status) = ButtonStatus::try_from_raw(evt.value) {
            let _ = ty.send(DeviceEvent::Button {
                time: seconds(evt.time),
                code: ButtonCode::from_raw(evt.code, *rotation, *button_scheme),
                status: button_status,
            });
        }
    }
}

pub fn handle_msc(evt: RawInputEvent, ty: &Sender<DeviceEvent>) {
    if evt.value >= MSC_RAW_GSENSOR_PORTRAIT_DOWN && evt.value <= MSC_RAW_GSENSOR_LANDSCAPE_LEFT {
        let next_rotation = GYROSCOPE_ROTATIONS
            .iter()
            .position(|&v| v == evt.value)
            .map(|i| CURRENT_DEVICE.transformed_gyroscope_rotation(i as i8));
        if let Some(next_rotation) = next_rotation {
            ty.send(DeviceEvent::RotateScreen(next_rotation)).ok();
        }
    }
}
