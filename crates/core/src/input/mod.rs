use crate::device::CURRENT_DEVICE;
use crate::framebuffer::Display;
use crate::geom::{LinearDir, Point};
use crate::settings::ButtonScheme;
use anyhow::{Context, Error};
use std::ffi::CString;
use std::fs::File;
use std::io::Read;
use std::mem::{self, MaybeUninit};
use std::os::unix::io::AsRawFd;
use std::ptr;
use std::slice;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub mod gestures;
pub mod handlers;

// Event types
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;

// Event codes
pub const ABS_MT_TRACKING_ID: u16 = 0x39;
pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_PRESSURE: u16 = 0x3a;
pub const ABS_MT_TOUCH_MAJOR: u16 = 0x30;
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_PRESSURE: u16 = 0x18;
pub const MSC_RAW: u16 = 0x03;
pub const SYN_REPORT: u16 = 0x00;
pub const SYN_MT_REPORT: u16 = 0x02;

// Event values
pub const MSC_RAW_GSENSOR_PORTRAIT_DOWN: i32 = 0x17;
pub const MSC_RAW_GSENSOR_PORTRAIT_UP: i32 = 0x18;
pub const MSC_RAW_GSENSOR_LANDSCAPE_RIGHT: i32 = 0x19;
pub const MSC_RAW_GSENSOR_LANDSCAPE_LEFT: i32 = 0x1a;
// pub const MSC_RAW_GSENSOR_BACK: i32 = 0x1b;
// pub const MSC_RAW_GSENSOR_FRONT: i32 = 0x1c;

// The indices of this clockwise ordering of the sensor values match the Forma's rotation values.
pub const GYROSCOPE_ROTATIONS: [i32; 4] = [
    MSC_RAW_GSENSOR_LANDSCAPE_LEFT,
    MSC_RAW_GSENSOR_PORTRAIT_UP,
    MSC_RAW_GSENSOR_LANDSCAPE_RIGHT,
    MSC_RAW_GSENSOR_PORTRAIT_DOWN,
];

pub const VAL_RELEASE: i32 = 0;
pub const VAL_PRESS: i32 = 1;
pub const VAL_REPEAT: i32 = 2;

// Key codes
pub const KEY_POWER: u16 = 116;
pub const KEY_HOME: u16 = 102;
pub const KEY_LIGHT: u16 = 90;
pub const KEY_BACKWARD: u16 = 193;
pub const KEY_FORWARD: u16 = 194;
pub const PEN_ERASE: u16 = 331;
pub const PEN_HIGHLIGHT: u16 = 332;
pub const SLEEP_COVER: [u16; 2] = [59, 35];

// Synthetic touch button
pub const BTN_TOUCH: u16 = 325;
// Synthetic touch button
pub const BTN_TOUCH_2: u16 = 330;

// The following key codes are fake, and are used to support
// software toggles within this design
pub const KEY_ROTATE_DISPLAY: u16 = 0xffff;
pub const KEY_BUTTON_SCHEME: u16 = 0xfffe;

pub const SINGLE_TOUCH_CODES: TouchCodes = TouchCodes {
    pressure: ABS_PRESSURE,
    x: ABS_X,
    y: ABS_Y,
};

pub const MULTI_TOUCH_CODES_A: TouchCodes = TouchCodes {
    pressure: ABS_MT_TOUCH_MAJOR,
    x: ABS_MT_POSITION_X,
    y: ABS_MT_POSITION_Y,
};

pub const MULTI_TOUCH_CODES_B: TouchCodes = TouchCodes {
    pressure: ABS_MT_PRESSURE,
    ..MULTI_TOUCH_CODES_A
};

pub const MULTI_TOUCH_CODES_SNOW: TouchCodes = TouchCodes {
    pressure: ABS_PRESSURE,
    ..MULTI_TOUCH_CODES_A
};

#[repr(C)]
#[derive(Clone)]
pub struct RawInputEvent {
    pub time: libc::timeval,
    pub kind: u16, // type
    pub code: u16,
    pub value: i32,
}

#[cfg(feature = "sim")]
impl RawInputEvent {
    pub fn new(timestamp_ms: u32, kind: u16, code: u16, value: i32) -> Self {
        Self {
            time: libc::timeval {
                tv_sec: timestamp_ms as i64 / 1000,
                tv_usec: timestamp_ms as i64 % 1000 * 1000,
            },
            kind,
            code,
            value,
        }
    }
}

// Handle different touch protocols
#[derive(Debug)]
pub struct TouchCodes {
    pressure: u16,
    x: u16,
    y: u16,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TouchProto {
    Single,
    MultiA,
    MultiB, // Pressure won't indicate a finger release.
    MultiC,
    MultiSnow,
    #[cfg(feature = "sim")]
    MultiSim,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FingerStatus {
    Down,
    Motion,
    Up,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ButtonStatus {
    Pressed,
    Released,
    Repeated,
}

impl ButtonStatus {
    pub fn try_from_raw(value: i32) -> Option<ButtonStatus> {
        match value {
            VAL_RELEASE => Some(ButtonStatus::Released),
            VAL_PRESS => Some(ButtonStatus::Pressed),
            VAL_REPEAT => Some(ButtonStatus::Repeated),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ButtonCode {
    Power,
    Home,
    Light,
    Backward,
    Forward,
    Erase,
    Highlight,
    Raw(u16),
}

impl ButtonCode {
    fn from_raw(code: u16, rotation: i8, button_scheme: ButtonScheme) -> ButtonCode {
        match code {
            KEY_POWER => ButtonCode::Power,
            KEY_HOME => ButtonCode::Home,
            KEY_LIGHT => ButtonCode::Light,
            KEY_BACKWARD => resolve_button_direction(LinearDir::Backward, rotation, button_scheme),
            KEY_FORWARD => resolve_button_direction(LinearDir::Forward, rotation, button_scheme),
            PEN_ERASE => ButtonCode::Erase,
            PEN_HIGHLIGHT => ButtonCode::Highlight,
            _ => ButtonCode::Raw(code),
        }
    }
}

fn resolve_button_direction(
    mut direction: LinearDir,
    rotation: i8,
    button_scheme: ButtonScheme,
) -> ButtonCode {
    if (CURRENT_DEVICE.should_invert_buttons(rotation)) ^ (button_scheme == ButtonScheme::Inverted)
    {
        direction = direction.opposite();
    }

    if direction == LinearDir::Forward {
        return ButtonCode::Forward;
    }

    ButtonCode::Backward
}

pub fn display_rotate_event(n: i8) -> RawInputEvent {
    let mut tp = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    unsafe {
        libc::gettimeofday(&mut tp, ptr::null_mut());
    }
    RawInputEvent {
        time: tp,
        kind: EV_KEY,
        code: KEY_ROTATE_DISPLAY,
        value: n as i32,
    }
}

pub fn button_scheme_event(v: i32) -> RawInputEvent {
    let mut tp = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    unsafe {
        libc::gettimeofday(&mut tp, ptr::null_mut());
    }
    RawInputEvent {
        time: tp,
        kind: EV_KEY,
        code: KEY_BUTTON_SCHEME,
        value: v,
    }
}

/// Device-related events.
#[derive(Debug, Copy, Clone)]
pub enum DeviceEvent {
    /// Finger-related events (pressed, moved...)
    Finger {
        id: i32,
        time: f64,
        status: FingerStatus,
        position: Point,
    },
    /// Button-related events (pressed, released...)
    Button {
        time: f64,
        code: ButtonCode,
        status: ButtonStatus,
    },
    /// Device plugged to source `PowerSource`.
    Plug(PowerSource),
    /// Device unplugged from source `PowerSource`.
    Unplug(PowerSource),
    /// Screen rotated.
    RotateScreen(i8),
    /// The magnetic cover has been closed.
    CoverOn,
    /// The magnetic cover has been lifted.
    CoverOff,
    /// The network interface is up.
    NetUp,
    /// The user has just interacted with the device (finger up or down, button pressed...)
    UserActivity,
}

/// The source that is powering the (plugged) device.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PowerSource {
    /// A PC.
    Host,
    /// A wall charger.
    Wall,
}

pub fn seconds(time: libc::timeval) -> f64 {
    time.tv_sec as f64 + time.tv_usec as f64 / 1e6
}

pub fn raw_events(paths: Vec<String>) -> (Sender<RawInputEvent>, Receiver<RawInputEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    thread::spawn(move || parse_raw_events(&paths, &tx));
    (tx2, rx)
}

pub fn parse_raw_events(paths: &[String], tx: &Sender<RawInputEvent>) -> Result<(), Error> {
    let mut files = Vec::new();
    let mut pfds = Vec::new();

    for path in paths.iter() {
        let file = File::open(path).with_context(|| format!("can't open input file {}", path))?;
        let fd = file.as_raw_fd();
        files.push(file);
        pfds.push(libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        });
    }

    loop {
        let ret = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, -1) };
        if ret < 0 {
            break;
        }
        for (pfd, mut file) in pfds.iter().zip(&files) {
            if pfd.revents & libc::POLLIN != 0 {
                let mut input_event = MaybeUninit::<RawInputEvent>::uninit();
                unsafe {
                    let event_slice = slice::from_raw_parts_mut(
                        input_event.as_mut_ptr() as *mut u8,
                        mem::size_of::<RawInputEvent>(),
                    );
                    if file.read_exact(event_slice).is_err() {
                        break;
                    }
                    tx.send(input_event.assume_init()).ok();
                }
            }
        }
    }

    Ok(())
}

pub fn usb_events() -> Receiver<DeviceEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || parse_usb_events(&tx));
    rx
}

fn parse_usb_events(tx: &Sender<DeviceEvent>) {
    let path = CString::new("/tmp/nickel-hardware-status").unwrap();
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_NONBLOCK | libc::O_RDWR) };

    if fd < 0 {
        return;
    }

    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };

    const BUF_LEN: usize = 256;

    loop {
        let ret = unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, -1) };

        if ret < 0 {
            break;
        }

        let buf = CString::new(vec![1; BUF_LEN]).unwrap();
        let c_buf = buf.into_raw();

        if pfd.revents & libc::POLLIN != 0 {
            let n = unsafe { libc::read(fd, c_buf as *mut libc::c_void, BUF_LEN as libc::size_t) };
            let buf = unsafe { CString::from_raw(c_buf) };
            if n > 0 {
                if let Ok(s) = buf.to_str() {
                    for msg in s[..n as usize].lines() {
                        if msg == "usb plug add" {
                            tx.send(DeviceEvent::Plug(PowerSource::Host)).ok();
                        } else if msg == "usb plug remove" {
                            tx.send(DeviceEvent::Unplug(PowerSource::Host)).ok();
                        } else if msg == "usb ac add" {
                            tx.send(DeviceEvent::Plug(PowerSource::Wall)).ok();
                        } else if msg == "usb ac remove" {
                            tx.send(DeviceEvent::Unplug(PowerSource::Wall)).ok();
                        } else if msg.starts_with("network bound") {
                            tx.send(DeviceEvent::NetUp).ok();
                        }
                    }
                }
            } else {
                break;
            }
        }
    }
}

pub fn device_events(
    rx: Receiver<RawInputEvent>,
    display: Display,
    button_scheme: ButtonScheme,
) -> Receiver<DeviceEvent> {
    let (device_ev_tx, device_ev_rx) = mpsc::channel();

    thread::spawn(move || parse_device_events(&rx, &device_ev_tx, display, button_scheme));
    device_ev_rx
}

struct TouchState {
    position: Point,
    pressure: i32,
}

impl Default for TouchState {
    fn default() -> Self {
        TouchState {
            position: Point::default(),
            pressure: 0,
        }
    }
}

pub fn parse_device_events(
    rx: &Receiver<RawInputEvent>,
    ty: &Sender<DeviceEvent>,
    display: Display,
    button_scheme: ButtonScheme,
) {
    let proto = CURRENT_DEVICE.proto;

    let mut tc = match proto {
        TouchProto::Single => SINGLE_TOUCH_CODES,
        TouchProto::MultiA => MULTI_TOUCH_CODES_A,
        TouchProto::MultiB => MULTI_TOUCH_CODES_B,
        TouchProto::MultiC => MULTI_TOUCH_CODES_B,
        TouchProto::MultiSnow => MULTI_TOUCH_CODES_SNOW,
        #[cfg(feature = "sim")]
        TouchProto::MultiSim => MULTI_TOUCH_CODES_SNOW,
    };

    if CURRENT_DEVICE.should_swap_axes(display.rotation) {
        mem::swap(&mut tc.x, &mut tc.y);
    }

    (CURRENT_DEVICE.input_handler)(rx, ty, tc, proto, button_scheme, display);
}
