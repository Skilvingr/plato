use std::{fs::File, io::Write, mem, slice};

use kaesar_core::input::{
    RawInputEvent, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_TRACKING_ID, EV_ABS, EV_KEY,
    EV_SYN, KEY_POWER, SYN_REPORT,
};

pub fn write_events(ev_file: &mut File, evts: &[RawInputEvent], syn: RawInputEvent) {
    for ev in evts.iter().chain(&[syn]) {
        let ev = ev as *const RawInputEvent as *const u8;
        let buf = unsafe { slice::from_raw_parts(ev, mem::size_of::<RawInputEvent>()) };
        ev_file.write_all(buf).unwrap();
        ev_file.flush().unwrap();
    }
}

pub fn power_btn_up_down(ev_file: &mut File, timestamp_ms: u32, up: bool) {
    let syn = RawInputEvent::new(timestamp_ms + 1, EV_SYN, SYN_REPORT, 0);
    let evts = [RawInputEvent::new(
        timestamp_ms,
        EV_KEY,
        KEY_POWER,
        if up { 0 } else { 1 },
    )];

    write_events(ev_file, &evts, syn);
}

pub mod sane {
    use super::*;

    pub fn _finger_up_down(ev_file: &mut File, timestamp: u32, x: i32, y: i32, up: bool) {
        let syn = RawInputEvent::new(timestamp + 1, EV_SYN, SYN_REPORT, 0);
        let evts = [
            RawInputEvent::new(timestamp, EV_ABS, ABS_MT_TRACKING_ID, 42),
            RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_X, x),
            RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_Y, y),
        ];

        write_events(ev_file, if up { &[] } else { &evts }, syn);
    }

    pub fn _finger_move(ev_file: &mut File, timestamp: u32, x: i32, y: i32) {
        let syn = RawInputEvent::new(timestamp + 1, EV_SYN, SYN_REPORT, 0);
        let evts = [
            RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_X, x),
            RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_Y, y),
        ];

        write_events(ev_file, &evts, syn);
    }
}

pub mod snow {
    use kaesar_core::input::SYN_MT_REPORT;

    use super::*;

    pub fn _finger_up_down(
        ev_file: &mut File,
        timestamp: u32,
        x: i32,
        y: i32,
        up: bool,
        double: bool,
    ) {
        let syn = RawInputEvent::new(timestamp, EV_SYN, SYN_REPORT, 0);

        let evts = if double {
            vec![
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_TRACKING_ID, 42),
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_X, x - 100),
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_Y, y),
                RawInputEvent::new(timestamp, EV_SYN, SYN_MT_REPORT, 0),
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_TRACKING_ID, 43),
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_X, x + 100),
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_Y, y),
                RawInputEvent::new(timestamp, EV_SYN, SYN_MT_REPORT, 0),
            ]
        } else {
            vec![
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_TRACKING_ID, 42),
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_X, x),
                RawInputEvent::new(timestamp, EV_ABS, ABS_MT_POSITION_Y, y),
                RawInputEvent::new(timestamp, EV_SYN, SYN_MT_REPORT, 0),
            ]
        };

        write_events(ev_file, if up { &[] } else { &evts }, syn);
    }

    pub fn _finger_move(ev_file: &mut File, timestamp: u32, x: i32, y: i32, double: bool) {
        _finger_up_down(ev_file, timestamp, x, y, false, double);
    }
}
