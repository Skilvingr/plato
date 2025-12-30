use std::{fs::File, io::Write, mem, slice};

use plato_core::input::{
    InputEvent, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_PRESSURE, ABS_MT_TRACKING_ID, EV_ABS,
    EV_SYN, SYN_REPORT,
};

pub fn write_events(ev_file: &mut File, evts: &[InputEvent]) {
    for ev in evts {
        let ev = ev as *const InputEvent as *const u8;
        let buf = unsafe { slice::from_raw_parts(ev, mem::size_of::<InputEvent>()) };
        ev_file.write_all(buf).unwrap();
    }
}

pub fn mouse_btn_evt(ev_file: &mut File, timestamp: u32, x: i32, y: i32, up: bool) {
    let evts = [
        InputEvent::new(timestamp, EV_ABS, ABS_MT_TRACKING_ID, 42),
        InputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_X, x),
        InputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_Y, y),
        InputEvent::new(
            timestamp + 1,
            EV_ABS,
            ABS_MT_PRESSURE,
            if up { -1 } else { 5 },
        ),
        InputEvent::new(timestamp + 1, EV_SYN, SYN_REPORT, 0),
    ];

    write_events(ev_file, &evts);
}

pub fn mouse_move_evt(ev_file: &mut File, timestamp: u32, x: i32, y: i32) {
    let evts = [
        InputEvent::new(timestamp, EV_ABS, ABS_MT_TRACKING_ID, 42),
        InputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_X, x),
        InputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_Y, y),
        InputEvent::new(timestamp, EV_SYN + 1, SYN_REPORT, 0),
    ];

    write_events(ev_file, &evts);
}
