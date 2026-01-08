use kaesar_core::anyhow::{Context as ResultExt, Error};
use kaesar_core::chrono::{Local, Timelike};
use kaesar_core::context::Context;
use kaesar_core::device::{CURRENT_DEVICE, Orientation};
use kaesar_core::document::sys_info_as_html;
use kaesar_core::framebuffer::UpdateMode;
use kaesar_core::geom::{DiagDir, Rectangle, Region};
use kaesar_core::helpers::{load_toml, save_toml};
use kaesar_core::input::gestures::{GestureEvent, gesture_events};
use kaesar_core::input::{
    ButtonCode, ButtonStatus, DeviceEvent, PowerSource, VAL_PRESS, VAL_RELEASE,
};
use kaesar_core::input::{
    button_scheme_event, device_events, display_rotate_event, raw_events, usb_events,
};
use kaesar_core::peripherals::{usb, wifi};
use kaesar_core::settings::{ButtonScheme, IntermKind, RotationLock, SETTINGS_PATH, Settings};
use kaesar_core::tasks::{self, Task, TaskId};
use kaesar_core::view::calculator::Calculator;
use kaesar_core::view::common::{
    locate, locate_by_id, overlapping_rectangle, transfer_notifications,
};
use kaesar_core::view::common::{toggle_input_history_menu, toggle_keyboard_layout_menu};
use kaesar_core::view::dialog::Dialog;
use kaesar_core::view::dictionary::Dictionary as DictionaryApp;
use kaesar_core::view::frontlight::FrontlightWindow;
use kaesar_core::view::home::Home;
use kaesar_core::view::intermission::Intermission;
use kaesar_core::view::menu::{Menu, MenuKind};
use kaesar_core::view::notification::Notification;
use kaesar_core::view::reader::Reader;
use kaesar_core::view::rotation_values::RotationValues;
use kaesar_core::view::sketch::Sketch;
use kaesar_core::view::top_bar::TopBar;
use kaesar_core::view::touch_events::TouchEvents;
use kaesar_core::view::{
    AppCmd, EntryId, EntryKind, Event, RenderData, RenderQueue, UpdateData, View, ViewId,
};
use kaesar_core::view::{handle_event, process_render_queue, wait_for_all};
use kaesar_core::{ExitStatus, ssh};
use std::collections::VecDeque;
use std::env;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

pub const APP_NAME: &str = "Kaesar";

#[cfg(not(feature = "sim"))]
const TOUCH_INPUTS: [&str; 5] = [
    "/dev/input/by-path/platform-2-0010-event",
    "/dev/input/by-path/platform-1-0038-event",
    "/dev/input/by-path/platform-1-0010-event",
    "/dev/input/by-path/platform-0-0010-event",
    "/dev/input/event1",
];
const BUTTON_INPUTS: [&str; 4] = [
    "/dev/input/by-path/platform-gpio-keys-event",
    "/dev/input/by-path/platform-ntx_event0-event",
    "/dev/input/by-path/platform-mxckpd-event",
    "/dev/input/event0",
];
const POWER_INPUTS: [&str; 3] = [
    "/dev/input/by-path/platform-bd71828-pwrkey.6.auto-event",
    "/dev/input/by-path/platform-bd71828-pwrkey.4.auto-event",
    "/dev/input/by-path/platform-bd71828-pwrkey-event",
];

const KOBO_UPDATE_BUNDLE: &str = "/mnt/onboard/.kobo/KoboRoot.tgz";

const CLOCK_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const BATTERY_REFRESH_INTERVAL: Duration = Duration::from_mins(2);
const AUTO_SUSPEND_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const SUSPEND_WAIT_DELAY: Duration = Duration::from_secs(15);
const PREPARE_SUSPEND_WAIT_DELAY: Duration = Duration::from_secs(3);

struct HistoryItem {
    view: Box<dyn View>,
    rotation: i8,
    monochrome: bool,
    dithered: bool,
}

fn resume(
    id: TaskId,
    tasks: &mut Vec<Task>,
    view: &mut dyn View,
    hub: &Sender<Event>,
    rq: &mut RenderQueue,
    context: &mut Context,
) {
    if id == TaskId::Suspend {
        tasks.retain(|task| task.id != TaskId::Suspend);
        if context.settings.frontlight {
            let levels = context.settings.frontlight_levels;
            context.frontlight.set_warmth(levels.warmth);
            context.frontlight.set_intensity(levels.intensity);
        }
        if context.settings.wifi {
            wifi::set_wifi_tmp(true);
        }
    }
    if id == TaskId::Suspend || id == TaskId::PrepareSuspend {
        tasks.retain(|task| task.id != TaskId::PrepareSuspend);
        if let Some(index) = locate::<Intermission>(view) {
            let rect = *view.child(index).rect();
            view.children_mut().remove(index);
            rq.add(RenderData::expose(rect, UpdateMode::Full));
        }
        hub.send(Event::ClockTick).ok();
    }
}

fn power_off_or_reboot(
    view: &mut dyn View,
    history: &mut Vec<HistoryItem>,
    updating: &mut Vec<UpdateData>,
    context: &mut Context,
    power_off: bool,
) {
    let (tx, _rx) = mpsc::channel();
    view.handle_event(
        &Event::Back,
        &tx,
        &mut VecDeque::new(),
        &mut RenderQueue::new(),
        context,
    );
    while let Some(mut item) = history.pop() {
        item.view.handle_event(
            &Event::Back,
            &tx,
            &mut VecDeque::new(),
            &mut RenderQueue::new(),
            context,
        );
    }
    let interm = Intermission::new(
        context.fb.rect(),
        if power_off {
            IntermKind::PowerOff
        } else {
            IntermKind::Reboot
        },
        context,
    );
    wait_for_all(updating, context);
    interm.render(context.fb.as_mut(), *interm.rect(), &mut context.fonts);
    context.fb.update(interm.rect(), UpdateMode::Full).ok();
}

pub fn run(
    mut context: Context,
    initial_rotation: i8,

    // We need to issue a complete redraw after some operations on the window
    // holding the simulator. Such a redraw will be run on every message received
    // on this receiver.
    #[cfg(feature = "sim")] redraw_requested: std::sync::mpsc::Receiver<()>,
) -> Result<i32, Error> {
    let mut inactive_since = Instant::now();
    let mut exit_status = ExitStatus::Quit;

    context.plugged = context.battery.status().is_ok_and(|v| v[0].is_wired());

    if context.settings.import.startup_trigger {
        context.batch_import();
    }
    context.load_dictionaries();
    context.load_keyboard_layouts();

    let mut paths = Vec::new();

    #[cfg(feature = "sim")]
    paths.push("emulator_sysroot/sim-touch-evts".to_string());
    //paths.push("/dev/input/event16".to_string());

    #[cfg(not(feature = "sim"))]
    for ti in &TOUCH_INPUTS {
        if Path::new(ti).exists() {
            paths.push(ti.to_string());
            break;
        }
    }

    for bi in &BUTTON_INPUTS {
        if Path::new(bi).exists() {
            paths.push(bi.to_string());
            break;
        }
    }
    for pi in &POWER_INPUTS {
        if Path::new(pi).exists() {
            paths.push(pi.to_string());
            break;
        }
    }

    // Input events from the kernel
    let (raw_sender, raw_receiver) = raw_events(paths);

    // Processed device events
    let device_ev_rx = device_events(
        raw_receiver,
        context.display,
        context.settings.button_scheme,
    );

    // Final kaesar events
    let (kaesar_ev_tx, kaesar_ev_rx) = mpsc::channel();

    let tx2 = kaesar_ev_tx.clone();
    gesture_events(device_ev_rx, tx2);

    let usb_ev_rx = usb_events();

    let tx3 = kaesar_ev_tx.clone();
    thread::spawn(move || {
        while let Ok(evt) = usb_ev_rx.recv() {
            tx3.send(Event::Device(evt)).ok();
        }
    });

    let tx4 = kaesar_ev_tx.clone();
    thread::spawn(move || {
        tx4.send(Event::ClockTick).ok();
        thread::sleep(Duration::from_secs(
            60 - Local::now().time().second() as u64,
        ));

        loop {
            tx4.send(Event::ClockTick).ok();
            thread::sleep(CLOCK_REFRESH_INTERVAL);
        }
    });

    let tx5 = kaesar_ev_tx.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(BATTERY_REFRESH_INTERVAL);
            tx5.send(Event::BatteryTick).ok();
        }
    });

    if context.settings.auto_suspend > 0.0 {
        let tx6 = kaesar_ev_tx.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(AUTO_SUSPEND_REFRESH_INTERVAL);
                tx6.send(Event::MightSuspend).ok();
            }
        });
    }

    context.fb.set_inverted(context.settings.inverted);

    // Avoid enabling wifi when run in the simulator.
    #[cfg(not(feature = "sim"))]
    wifi::set_wifi_tmp(context.settings.wifi);

    if context.settings.frontlight {
        let levels = context.settings.frontlight_levels;
        context.frontlight.set_warmth(levels.warmth);
        context.frontlight.set_intensity(levels.intensity);
    } else {
        context.frontlight.set_intensity(0.0);
        context.frontlight.set_warmth(0.0);
    }

    if context.settings.ssh {
        kaesar_core::ssh::set_ssh(true, &mut context, &kaesar_ev_tx);
    }

    let mut tasks: Vec<Task> = Vec::new();
    let mut history: Vec<HistoryItem> = Vec::new();
    let mut rq = RenderQueue::new();

    let mut view: Box<dyn View> = Box::new(Home::new(
        context.fb.rect(),
        &kaesar_ev_tx,
        &mut rq,
        &mut context,
    )?);

    // let mut view: Box<dyn View> =
    //     Box::new(TouchEvents::new(context.fb.rect(), &mut rq, &mut context));

    // let info = Info {
    //     file: FileInfo {
    //         path: PathBuf::from_str("l78.pdf").unwrap(),
    //         ..Default::default()
    //     },
    //     ..Default::default()
    // };
    // let mut view: Box<dyn View> =
    //     Box::new(Reader::new(context.fb.rect(), info, &kaesar_ev_tx, &mut context).unwrap());

    let mut updating = Vec::new();
    let current_dir = env::current_dir()?;

    println!(
        "{} is running on a Kobo {}.",
        APP_NAME, CURRENT_DEVICE.model
    );
    println!(
        "The framebuffer resolution is {} by {}.",
        context.fb.rect().width(),
        context.fb.rect().height()
    );

    let mut bus = VecDeque::with_capacity(4);

    kaesar_ev_tx.send(Event::WakeUp).ok();

    println!("Entering main loop");
    while let Ok(evt) = kaesar_ev_rx.recv() {
        // Full redraw when requested by the simulator.
        #[cfg(feature = "sim")]
        if let Ok(_) = redraw_requested.try_recv() {
            rq.add(RenderData::new(
                view.id(),
                context.fb.rect(),
                UpdateMode::Full,
            ));
        }

        match evt {
            Event::Device(de) => match de {
                DeviceEvent::Button {
                    code: ButtonCode::Power,
                    status: ButtonStatus::Released,
                    ..
                } => {
                    if context.shared || context.covered {
                        continue;
                    }

                    if tasks.iter().any(|task| task.id == TaskId::PrepareSuspend) {
                        resume(
                            TaskId::PrepareSuspend,
                            &mut tasks,
                            view.as_mut(),
                            &kaesar_ev_tx,
                            &mut rq,
                            &mut context,
                        );
                    } else if tasks.iter().any(|task| task.id == TaskId::Suspend) {
                        resume(
                            TaskId::Suspend,
                            &mut tasks,
                            view.as_mut(),
                            &kaesar_ev_tx,
                            &mut rq,
                            &mut context,
                        );
                    } else {
                        view.handle_event(
                            &Event::Suspend,
                            &kaesar_ev_tx,
                            &mut bus,
                            &mut rq,
                            &mut context,
                        );
                        let interm =
                            Intermission::new(context.fb.rect(), IntermKind::Suspend, &context);
                        rq.add(RenderData::new(
                            interm.id(),
                            *interm.rect(),
                            UpdateMode::Full,
                        ));
                        tasks::schedule_task(
                            TaskId::PrepareSuspend,
                            Event::PrepareSuspend,
                            PREPARE_SUSPEND_WAIT_DELAY,
                            &kaesar_ev_tx,
                            &mut tasks,
                        );
                        view.children_mut().push(Box::new(interm) as Box<dyn View>);
                    }
                }
                DeviceEvent::Button {
                    code: ButtonCode::Light,
                    status: ButtonStatus::Pressed,
                    ..
                } => {
                    kaesar_ev_tx.send(Event::ToggleFrontlight).ok();
                }
                DeviceEvent::CoverOn => {
                    if context.covered {
                        continue;
                    }

                    context.covered = true;

                    if !context.settings.sleep_cover
                        || context.shared
                        || tasks.iter().any(|task| {
                            task.id == TaskId::PrepareSuspend || task.id == TaskId::Suspend
                        })
                    {
                        continue;
                    }

                    view.handle_event(
                        &Event::Suspend,
                        &kaesar_ev_tx,
                        &mut bus,
                        &mut rq,
                        &mut context,
                    );
                    let interm =
                        Intermission::new(context.fb.rect(), IntermKind::Suspend, &context);
                    rq.add(RenderData::new(
                        interm.id(),
                        *interm.rect(),
                        UpdateMode::Full,
                    ));
                    tasks::schedule_task(
                        TaskId::PrepareSuspend,
                        Event::PrepareSuspend,
                        PREPARE_SUSPEND_WAIT_DELAY,
                        &kaesar_ev_tx,
                        &mut tasks,
                    );
                    view.children_mut().push(Box::new(interm) as Box<dyn View>);
                }
                DeviceEvent::CoverOff => {
                    if !context.covered {
                        continue;
                    }

                    context.covered = false;

                    if context.shared || !context.settings.sleep_cover {
                        continue;
                    }

                    if tasks.iter().any(|task| task.id == TaskId::PrepareSuspend) {
                        resume(
                            TaskId::PrepareSuspend,
                            &mut tasks,
                            view.as_mut(),
                            &kaesar_ev_tx,
                            &mut rq,
                            &mut context,
                        );
                    } else if tasks.iter().any(|task| task.id == TaskId::Suspend) {
                        resume(
                            TaskId::Suspend,
                            &mut tasks,
                            view.as_mut(),
                            &kaesar_ev_tx,
                            &mut rq,
                            &mut context,
                        );
                    }
                }
                DeviceEvent::NetUp => {
                    if tasks
                        .iter()
                        .any(|task| task.id == TaskId::PrepareSuspend || task.id == TaskId::Suspend)
                    {
                        continue;
                    }
                    let ip = Command::new("scripts/ip.sh")
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
                        .unwrap_or_default();
                    let essid = Command::new("scripts/essid.sh")
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
                        .unwrap_or_default();
                    let notif = Notification::new(
                        format!("Network is up ({}, {}).", ip, essid),
                        None,
                        &kaesar_ev_tx,
                        &mut rq,
                        &mut context,
                    );
                    context.online = true;
                    view.children_mut().push(Box::new(notif) as Box<dyn View>);
                    if view.is::<Home>() {
                        view.handle_event(&evt, &kaesar_ev_tx, &mut bus, &mut rq, &mut context);
                    } else if let Some(entry) =
                        history.get_mut(0).filter(|entry| entry.view.is::<Home>())
                    {
                        let (tx, _rx) = mpsc::channel();
                        entry.view.handle_event(
                            &evt,
                            &tx,
                            &mut VecDeque::new(),
                            &mut RenderQueue::new(),
                            &mut context,
                        );
                    }
                }
                DeviceEvent::Plug(power_source) => {
                    if context.plugged {
                        continue;
                    }

                    context.plugged = true;

                    if context.covered {
                        continue;
                    }

                    match power_source {
                        PowerSource::Wall => {
                            if tasks.iter().any(|task| task.id == TaskId::Suspend) {
                                continue;
                            }
                        }
                        PowerSource::Host => {
                            if tasks.iter().any(|task| task.id == TaskId::PrepareSuspend) {
                                resume(
                                    TaskId::PrepareSuspend,
                                    &mut tasks,
                                    view.as_mut(),
                                    &kaesar_ev_tx,
                                    &mut rq,
                                    &mut context,
                                );
                            } else if tasks.iter().any(|task| task.id == TaskId::Suspend) {
                                resume(
                                    TaskId::Suspend,
                                    &mut tasks,
                                    view.as_mut(),
                                    &kaesar_ev_tx,
                                    &mut rq,
                                    &mut context,
                                );
                            }

                            if context.settings.auto_share {
                                kaesar_ev_tx.send(Event::PrepareShare).ok();
                            } else {
                                let dialog = Dialog::new(
                                    ViewId::ShareDialog,
                                    Some(Event::PrepareShare),
                                    "Share storage via USB?".to_string(),
                                    &mut context,
                                );
                                rq.add(RenderData::new(
                                    dialog.id(),
                                    *dialog.rect(),
                                    UpdateMode::Gui,
                                ));
                                view.children_mut().push(Box::new(dialog) as Box<dyn View>);
                            }

                            inactive_since = Instant::now();
                        }
                    }

                    let _ = kaesar_ev_tx.send(Event::BatteryTick);
                }
                DeviceEvent::Unplug(..) => {
                    if !context.plugged {
                        continue;
                    }

                    if context.shared {
                        context.shared = false;
                        if let Some(_exit_status) = usb::manage_usb(false, &mut context) {
                            exit_status = _exit_status;
                            break;
                        }

                        env::set_current_dir(&current_dir)
                            .map_err(|e| {
                                eprintln!(
                                    "Can't set current directory to {}: {:#}.",
                                    current_dir.display(),
                                    e
                                )
                            })
                            .ok();
                        let path = Path::new(SETTINGS_PATH);
                        if let Ok(settings) = load_toml::<Settings, _>(path)
                            .map_err(|e| eprintln!("Can't load settings: {:#}.", e))
                        {
                            context.settings = settings;
                        }
                        if context.settings.wifi {
                            wifi::set_wifi_tmp(true);
                        }
                        if context.settings.frontlight {
                            let levels = context.settings.frontlight_levels;
                            context.frontlight.set_warmth(levels.warmth);
                            context.frontlight.set_intensity(levels.intensity);
                        }
                        if let Some(index) = locate::<Intermission>(view.as_ref()) {
                            let rect = *view.child(index).rect();
                            view.children_mut().remove(index);
                            rq.add(RenderData::expose(rect, UpdateMode::Full));
                        }
                        if Path::new(KOBO_UPDATE_BUNDLE).exists() {
                            kaesar_ev_tx.send(Event::Select(EntryId::Reboot)).ok();
                        }
                        context.library.reload();
                        if context.settings.import.unshare_trigger {
                            context.batch_import();
                        }
                        view.handle_event(
                            &Event::Reseed,
                            &kaesar_ev_tx,
                            &mut bus,
                            &mut rq,
                            &mut context,
                        );
                    } else {
                        context.plugged = false;

                        if tasks.iter().any(|task| task.id == TaskId::Suspend) {
                            if !context.covered {
                                resume(
                                    TaskId::Suspend,
                                    &mut tasks,
                                    view.as_mut(),
                                    &kaesar_ev_tx,
                                    &mut rq,
                                    &mut context,
                                );
                            }
                        }
                    }

                    let _ = kaesar_ev_tx.send(Event::BatteryTick);
                }
                DeviceEvent::RotateScreen(n) => {
                    if context.shared
                        || tasks.iter().any(|task| {
                            task.id == TaskId::PrepareSuspend || task.id == TaskId::Suspend
                        })
                    {
                        continue;
                    }

                    if view.is::<RotationValues>() {
                        println!("Gyro rotation: {}", n);
                    }

                    if let Some(rotation_lock) = context.settings.rotation_lock {
                        let orientation = CURRENT_DEVICE.orientation(n);
                        if rotation_lock == RotationLock::Current
                            || (rotation_lock == RotationLock::Portrait
                                && orientation == Orientation::Landscape)
                            || (rotation_lock == RotationLock::Landscape
                                && orientation == Orientation::Portrait)
                        {
                            continue;
                        }
                    }

                    kaesar_ev_tx.send(Event::Select(EntryId::Rotate(n))).ok();
                }
                DeviceEvent::UserActivity if context.settings.auto_suspend > 0.0 => {
                    inactive_since = Instant::now();
                }
                _ => {
                    handle_event(
                        view.as_mut(),
                        &evt,
                        &kaesar_ev_tx,
                        &mut bus,
                        &mut rq,
                        &mut context,
                    );
                }
            },
            Event::SshUp(message) => {
                let notif = Notification::new(
                    message.to_string(),
                    None,
                    &kaesar_ev_tx,
                    &mut rq,
                    &mut context,
                );

                view.children_mut().push(Box::new(notif) as Box<dyn View>);
                if view.is::<Home>() {
                    view.handle_event(&evt, &kaesar_ev_tx, &mut bus, &mut rq, &mut context);
                } else if let Some(entry) =
                    history.get_mut(0).filter(|entry| entry.view.is::<Home>())
                {
                    let (tx, _rx) = mpsc::channel();
                    entry.view.handle_event(
                        &evt,
                        &tx,
                        &mut VecDeque::new(),
                        &mut RenderQueue::new(),
                        &mut context,
                    );
                }
            }
            Event::BatteryTick => {
                if tasks
                    .iter()
                    .any(|task| task.id == TaskId::PrepareSuspend || task.id == TaskId::Suspend)
                {
                    continue;
                }
                if let Ok(capacity) = context.battery.capacity().map(|v| v[0]) {
                    if capacity < context.settings.battery.power_off {
                        power_off_or_reboot(
                            view.as_mut(),
                            &mut history,
                            &mut updating,
                            &mut context,
                            true,
                        );
                        exit_status = ExitStatus::PowerOff;
                        break;
                    } else if capacity < context.settings.battery.warn {
                        let notif = Notification::new(
                            "The battery capacity is getting low.".to_string(),
                            None,
                            &kaesar_ev_tx,
                            &mut rq,
                            &mut context,
                        );
                        view.children_mut().push(Box::new(notif) as Box<dyn View>);
                    }

                    if let Some(top_bar) = view
                        .children_mut()
                        .iter_mut()
                        .find(|c| c.is::<TopBar>())
                        .and_then(|tb| tb.downcast_mut::<TopBar>())
                    {
                        top_bar.reseed(Some(capacity), None, &mut rq, &mut context);
                    }
                }
            }
            Event::PrepareSuspend => {
                tasks.retain(|task| task.id != TaskId::PrepareSuspend);
                wait_for_all(&mut updating, &mut context);
                let path = Path::new(SETTINGS_PATH);
                save_toml(&context.settings, path)
                    .map_err(|e| eprintln!("Can't save settings: {:#}.", e))
                    .ok();
                context.library.flush();

                if context.settings.frontlight {
                    context.settings.frontlight_levels = context.frontlight.levels();
                    context.frontlight.set_intensity(0.0);
                    context.frontlight.set_warmth(0.0);
                }

                wifi::set_wifi_tmp(false);
                context.online = false;

                // https://github.com/koreader/koreader/commit/71afe36
                tasks::schedule_task(
                    TaskId::Suspend,
                    Event::Suspend,
                    SUSPEND_WAIT_DELAY,
                    &kaesar_ev_tx,
                    &mut tasks,
                );
            }
            Event::Suspend => {
                if context.settings.auto_power_off > 0.0 {
                    context.rtc.iter().for_each(|rtc| {
                        rtc.set_alarm(context.settings.auto_power_off)
                            .map_err(|e| eprintln!("Can't set alarm: {:#}.", e))
                            .ok();
                    });
                }
                let before = Local::now();
                println!(
                    "{}",
                    before.format("Went to sleep on %B %-d, %Y at %H:%M:%S.")
                );
                Command::new("scripts/suspend.sh").status().ok();
                let after = Local::now();
                println!("{}", after.format("Woke up on %B %-d, %Y at %H:%M:%S."));
                Command::new("scripts/resume.sh").status().ok();
                inactive_since = Instant::now();
                // If the wake is legitimate, the task will be cancelled by `resume`.
                tasks::schedule_task(
                    TaskId::Suspend,
                    Event::Suspend,
                    SUSPEND_WAIT_DELAY,
                    &kaesar_ev_tx,
                    &mut tasks,
                );
                if context.settings.auto_power_off > 0.0 {
                    let dur = kaesar_core::chrono::Duration::seconds(
                        (86_400.0 * context.settings.auto_power_off) as i64,
                    );
                    if let Some(fired) = context.rtc.as_ref().and_then(|rtc| {
                        rtc.alarm()
                            .map_err(|e| eprintln!("Can't get alarm: {:#}", e))
                            .map(|rwa| {
                                !rwa.enabled()
                                    || (rwa.year() <= 1970
                                        && ((after - before) - dur).num_seconds().abs() < 3)
                            })
                            .ok()
                    }) {
                        if fired {
                            power_off_or_reboot(
                                view.as_mut(),
                                &mut history,
                                &mut updating,
                                &mut context,
                                true,
                            );
                            exit_status = ExitStatus::PowerOff;
                            break;
                        } else {
                            context.rtc.iter().for_each(|rtc| {
                                rtc.disable_alarm()
                                    .map_err(|e| eprintln!("Can't disable alarm: {:#}.", e))
                                    .ok();
                            });
                        }
                    }
                }
            }
            Event::PrepareShare => {
                if context.shared {
                    continue;
                }

                tasks.clear();
                view.handle_event(&Event::Back, &kaesar_ev_tx, &mut bus, &mut rq, &mut context);
                while let Some(mut item) = history.pop() {
                    item.view.handle_event(
                        &Event::Back,
                        &kaesar_ev_tx,
                        &mut bus,
                        &mut rq,
                        &mut context,
                    );
                    if item.rotation != context.display.rotation {
                        wait_for_all(&mut updating, &mut context);
                        if let Ok(dims) = context.fb.set_rotation(item.rotation) {
                            raw_sender.send(display_rotate_event(item.rotation)).ok();
                            context.display.rotation = item.rotation;
                            context.display.dims = dims;
                        }
                    }
                    view = item.view;
                }
                let path = Path::new(SETTINGS_PATH);
                save_toml(&context.settings, path)
                    .map_err(|e| eprintln!("Can't save settings: {:#}.", e))
                    .ok();
                context.library.flush();

                if context.settings.frontlight {
                    context.settings.frontlight_levels = context.frontlight.levels();
                    context.frontlight.set_intensity(0.0);
                    context.frontlight.set_warmth(0.0);
                }
                if context.settings.wifi {
                    wifi::set_wifi_tmp(false);
                    context.online = false;
                }

                let interm = Intermission::new(context.fb.rect(), IntermKind::Share, &context);
                rq.add(RenderData::new(
                    interm.id(),
                    *interm.rect(),
                    UpdateMode::Full,
                ));
                view.children_mut().push(Box::new(interm) as Box<dyn View>);
                kaesar_ev_tx.send(Event::Share).ok();
            }
            Event::Share => {
                if context.shared {
                    continue;
                }

                context.shared = true;

                if let Some(_exit_status) = usb::manage_usb(true, &mut context) {
                    exit_status = _exit_status;
                    break;
                }
            }
            Event::Gesture(ref ge) => {
                match ge {
                    GestureEvent::HoldButtonLong(ButtonCode::Power) => {
                        power_off_or_reboot(
                            view.as_mut(),
                            &mut history,
                            &mut updating,
                            &mut context,
                            true,
                        );
                        exit_status = ExitStatus::PowerOff;
                        break;
                    }
                    GestureEvent::MultiTap(points) => {
                        let mut points = points.clone();

                        if points[0].x > points[1].x {
                            points.swap(0, 1);
                        }
                        let rect = context.fb.rect();
                        let r1 = Region::from_point(
                            points[0],
                            rect,
                            context.settings.reader.strip_width,
                            context.settings.reader.corner_width,
                        );
                        let r2 = Region::from_point(
                            points[1],
                            rect,
                            context.settings.reader.strip_width,
                            context.settings.reader.corner_width,
                        );
                        match (r1, r2) {
                            (
                                Region::Corner(DiagDir::SouthWest),
                                Region::Corner(DiagDir::NorthEast),
                            ) => {
                                rq.add(RenderData::new(
                                    view.id(),
                                    context.fb.rect(),
                                    UpdateMode::Full,
                                ));
                            }
                            (
                                Region::Corner(DiagDir::NorthWest),
                                Region::Corner(DiagDir::SouthEast),
                            ) => {
                                kaesar_ev_tx
                                    .send(Event::Select(EntryId::TakeScreenshot))
                                    .ok();
                            }
                            _ => (),
                        }
                    }
                    _ => {}
                }

                // bubble up gesture event
                handle_event(
                    view.as_mut(),
                    &evt,
                    &kaesar_ev_tx,
                    &mut bus,
                    &mut rq,
                    &mut context,
                );
            }
            Event::ToggleFrontlight => {
                context.set_frontlight(!context.settings.frontlight);
                view.handle_event(
                    &Event::ToggleFrontlight,
                    &kaesar_ev_tx,
                    &mut bus,
                    &mut rq,
                    &mut context,
                );
            }
            Event::Open(info) => {
                let rotation = context.display.rotation;
                let dithered = context.fb.dithered();
                if let Some(reader_info) = info.reader.as_ref() {
                    if let Some(n) = reader_info
                        .rotation
                        .map(|n| CURRENT_DEVICE.from_canonical(n))
                    {
                        if CURRENT_DEVICE.orientation(n) != CURRENT_DEVICE.orientation(rotation) {
                            wait_for_all(&mut updating, &mut context);
                            if let Ok(dims) = context.fb.set_rotation(n) {
                                raw_sender.send(display_rotate_event(n)).ok();
                                context.display.rotation = n;
                                context.display.dims = dims;
                            }
                        }
                    }
                    context.fb.set_dithered(reader_info.dithered);
                } else {
                    context.fb.set_dithered(
                        context
                            .settings
                            .reader
                            .dithered_kinds
                            .contains(&info.file.kind),
                    );
                }
                let path = info.file.path.clone();
                if let Some(r) = Reader::new(context.fb.rect(), *info, &kaesar_ev_tx, &mut context)
                {
                    let mut next_view = Box::new(r) as Box<dyn View>;
                    transfer_notifications(
                        view.as_mut(),
                        next_view.as_mut(),
                        &mut rq,
                        &mut context,
                    );
                    history.push(HistoryItem {
                        view,
                        rotation,
                        monochrome: context.fb.monochrome(),
                        dithered,
                    });
                    view = next_view;
                } else {
                    if context.display.rotation != rotation {
                        if let Ok(dims) = context.fb.set_rotation(rotation) {
                            raw_sender.send(display_rotate_event(rotation)).ok();
                            context.display.rotation = rotation;
                            context.display.dims = dims;
                        }
                    }
                    context.fb.set_dithered(dithered);
                    handle_event(
                        view.as_mut(),
                        &Event::Invalid(path),
                        &kaesar_ev_tx,
                        &mut bus,
                        &mut rq,
                        &mut context,
                    );
                }
            }
            Event::Select(EntryId::About) => {
                let dialog = Dialog::new(
                    ViewId::AboutDialog,
                    None,
                    format!("Kaesar {}", env!("CARGO_PKG_VERSION")),
                    &mut context,
                );
                rq.add(RenderData::new(
                    dialog.id(),
                    *dialog.rect(),
                    UpdateMode::Gui,
                ));
                view.children_mut().push(Box::new(dialog) as Box<dyn View>);
            }
            Event::Select(EntryId::SystemInfo) => {
                view.children_mut().retain(|child| !child.is::<Menu>());
                let html = sys_info_as_html(&context);
                let r =
                    Reader::from_html(context.fb.rect(), &html, None, &kaesar_ev_tx, &mut context);
                let mut next_view = Box::new(r) as Box<dyn View>;
                transfer_notifications(view.as_mut(), next_view.as_mut(), &mut rq, &mut context);
                history.push(HistoryItem {
                    view,
                    rotation: context.display.rotation,
                    monochrome: context.fb.monochrome(),
                    dithered: context.fb.dithered(),
                });
                view = next_view;
            }
            Event::OpenHtml(ref html, ref link_uri) => {
                view.children_mut().retain(|child| !child.is::<Menu>());
                let r = Reader::from_html(
                    context.fb.rect(),
                    html,
                    link_uri.as_deref(),
                    &kaesar_ev_tx,
                    &mut context,
                );
                let mut next_view = Box::new(r) as Box<dyn View>;
                transfer_notifications(view.as_mut(), next_view.as_mut(), &mut rq, &mut context);
                history.push(HistoryItem {
                    view,
                    rotation: context.display.rotation,
                    monochrome: context.fb.monochrome(),
                    dithered: context.fb.dithered(),
                });
                view = next_view;
            }
            Event::Select(EntryId::Launch(app_cmd)) => {
                view.children_mut().retain(|child| !child.is::<Menu>());
                let monochrome = context.fb.monochrome();
                let mut next_view: Box<dyn View> = match app_cmd {
                    AppCmd::Sketch => {
                        context.fb.set_monochrome(true);
                        Box::new(Sketch::new(context.fb.rect(), &mut rq, &mut context))
                    }
                    AppCmd::Calculator => Box::new(Calculator::new(
                        context.fb.rect(),
                        &kaesar_ev_tx,
                        &mut rq,
                        &mut context,
                    )?),
                    AppCmd::Dictionary {
                        ref query,
                        ref language,
                    } => Box::new(DictionaryApp::new(
                        context.fb.rect(),
                        query,
                        language,
                        &kaesar_ev_tx,
                        &mut rq,
                        &mut context,
                    )),
                    AppCmd::TouchEvents => {
                        Box::new(TouchEvents::new(context.fb.rect(), &mut rq, &mut context))
                    }
                    AppCmd::RotationValues => Box::new(RotationValues::new(
                        context.fb.rect(),
                        &mut rq,
                        &mut context,
                    )),
                };
                transfer_notifications(view.as_mut(), next_view.as_mut(), &mut rq, &mut context);
                history.push(HistoryItem {
                    view,
                    rotation: context.display.rotation,
                    monochrome,
                    dithered: context.fb.dithered(),
                });
                view = next_view;
            }
            Event::Back => {
                if let Some(item) = history.pop() {
                    view = item.view;
                    if item.monochrome != context.fb.monochrome() {
                        context.fb.set_monochrome(item.monochrome);
                    }
                    if item.dithered != context.fb.dithered() {
                        context.fb.set_dithered(item.dithered);
                    }
                    if CURRENT_DEVICE.orientation(item.rotation)
                        != CURRENT_DEVICE.orientation(context.display.rotation)
                    {
                        let _ = kaesar_ev_tx.send(Event::Select(EntryId::Rotate(item.rotation)));
                    } else {
                        view.handle_event(
                            &Event::Reseed,
                            &kaesar_ev_tx,
                            &mut bus,
                            &mut rq,
                            &mut context,
                        );
                    }
                } else if !view.is::<Home>() {
                    break;
                }
            }
            Event::TogglePresetMenu(rect, index) => {
                if let Some(index) = locate_by_id(view.as_ref(), ViewId::PresetMenu) {
                    let rect = *view.child(index).rect();
                    view.children_mut().remove(index);
                    rq.add(RenderData::expose(rect, UpdateMode::Gui));
                } else {
                    let preset_menu = Menu::new(
                        rect,
                        ViewId::PresetMenu,
                        MenuKind::Contextual,
                        vec![EntryKind::Command(
                            "Remove".to_string(),
                            EntryId::RemovePreset(index),
                        )],
                        &mut context,
                    );
                    rq.add(RenderData::new(
                        preset_menu.id(),
                        *preset_menu.rect(),
                        UpdateMode::Gui,
                    ));
                    view.children_mut()
                        .push(Box::new(preset_menu) as Box<dyn View>);
                }
            }
            Event::Show(ViewId::Frontlight) => {
                if !context.settings.frontlight {
                    context.set_frontlight(true);
                    view.handle_event(
                        &Event::ToggleFrontlight,
                        &kaesar_ev_tx,
                        &mut bus,
                        &mut rq,
                        &mut context,
                    );
                }
                let flw = FrontlightWindow::new(&mut context);
                rq.add(RenderData::new(flw.id(), *flw.rect(), UpdateMode::Gui));
                view.children_mut().push(Box::new(flw) as Box<dyn View>);
            }
            Event::ToggleInputHistoryMenu(id, rect) => {
                toggle_input_history_menu(view.as_mut(), id, rect, None, &mut rq, &mut context);
            }
            Event::ToggleNear(ViewId::KeyboardLayoutMenu, rect) => {
                toggle_keyboard_layout_menu(view.as_mut(), rect, None, &mut rq, &mut context);
            }
            Event::Close(ViewId::Frontlight) => {
                if let Some(index) = locate::<FrontlightWindow>(view.as_ref()) {
                    let rect = *view.child(index).rect();
                    view.children_mut().remove(index);
                    rq.add(RenderData::expose(rect, UpdateMode::Gui));
                }
            }
            Event::Close(id) => {
                if let Some(index) = locate_by_id(view.as_ref(), id) {
                    let rect = overlapping_rectangle(view.child(index));
                    rq.add(RenderData::expose(rect, UpdateMode::Gui));
                    view.children_mut().remove(index);
                }
            }
            Event::Select(EntryId::ToggleInverted) => {
                context.fb.toggle_inverted();
                context.settings.inverted = context.fb.inverted();
                rq.add(RenderData::new(
                    view.id(),
                    context.fb.rect(),
                    UpdateMode::Full,
                ));
            }
            Event::Select(EntryId::ToggleDithered) => {
                context.fb.toggle_dithered();
                rq.add(RenderData::new(
                    view.id(),
                    context.fb.rect(),
                    UpdateMode::Full,
                ));
            }
            Event::Select(EntryId::Rotate(n))
                if n != context.display.rotation && view.might_rotate() =>
            {
                wait_for_all(&mut updating, &mut context);
                if let Ok(dims) = context.fb.set_rotation(n) {
                    raw_sender.send(display_rotate_event(n)).ok();
                    context.display.rotation = n;
                    let fb_rect = Rectangle::from(dims);
                    if context.display.dims != dims {
                        context.display.dims = dims;
                        view.resize(fb_rect, &kaesar_ev_tx, &mut rq, &mut context);
                    } else {
                        rq.add(RenderData::new(
                            view.id(),
                            context.fb.rect(),
                            UpdateMode::Full,
                        ));
                    }
                }
            }
            Event::Select(EntryId::SetRotationLock(rotation_lock)) => {
                context.settings.rotation_lock = rotation_lock;
            }
            Event::Select(EntryId::SetButtonScheme(button_scheme)) => {
                context.settings.button_scheme = button_scheme;

                // Sending a pseudo event into the raw_events channel toggles the inversion in the device_events channel
                match button_scheme {
                    ButtonScheme::Natural => {
                        raw_sender.send(button_scheme_event(VAL_RELEASE)).ok();
                    }
                    ButtonScheme::Inverted => {
                        raw_sender.send(button_scheme_event(VAL_PRESS)).ok();
                    }
                }
            }
            Event::SetWifi(enable) => {
                wifi::set_wifi_perm(enable, &mut context);
            }
            Event::Select(EntryId::ToggleWifi) => {
                wifi::set_wifi_perm(!context.settings.wifi, &mut context);
            }
            Event::Select(EntryId::ToggleSSH) => {
                ssh::set_ssh(!context.settings.ssh, &mut context, &kaesar_ev_tx);
            }
            Event::Select(EntryId::TakeScreenshot) => {
                let name = Local::now().format("screenshot-%Y%m%d_%H%M%S.png");
                let msg = match context.fb.save(&name.to_string()) {
                    Err(e) => format!("{}", e),
                    Ok(_) => format!("Saved {}.", name),
                };
                let notif = Notification::new(msg, None, &kaesar_ev_tx, &mut rq, &mut context);
                view.children_mut().push(Box::new(notif) as Box<dyn View>);
            }
            Event::CheckFetcher(..)
            | Event::FetcherAddDocument(..)
            | Event::FetcherRemoveDocument(..)
            | Event::FetcherSearch { .. }
                if !view.is::<Home>() =>
            {
                if let Some(entry) = history.get_mut(0).filter(|entry| entry.view.is::<Home>()) {
                    let (tx, _rx) = mpsc::channel();
                    entry.view.handle_event(
                        &evt,
                        &tx,
                        &mut VecDeque::new(),
                        &mut RenderQueue::new(),
                        &mut context,
                    );
                }
            }
            Event::Notify(msg) => {
                let notif = Notification::new(msg, None, &kaesar_ev_tx, &mut rq, &mut context);
                view.children_mut().push(Box::new(notif) as Box<dyn View>);
            }
            Event::Select(EntryId::RestartApp) => {
                exit_status = ExitStatus::RestartApp;
                break;
            }
            Event::Select(EntryId::Reboot) => {
                power_off_or_reboot(
                    view.as_mut(),
                    &mut history,
                    &mut updating,
                    &mut context,
                    false,
                );
                exit_status = ExitStatus::Reboot;
                break;
            }
            Event::Select(EntryId::Quit) => {
                break;
            }
            Event::MightSuspend if context.settings.auto_suspend > 0.0 => {
                if context.shared
                    || tasks
                        .iter()
                        .any(|task| task.id == TaskId::PrepareSuspend || task.id == TaskId::Suspend)
                {
                    inactive_since = Instant::now();
                    continue;
                }
                let seconds = 60.0 * context.settings.auto_suspend;
                if inactive_since.elapsed() > Duration::from_secs_f32(seconds) {
                    view.handle_event(
                        &Event::Suspend,
                        &kaesar_ev_tx,
                        &mut bus,
                        &mut rq,
                        &mut context,
                    );
                    let interm =
                        Intermission::new(context.fb.rect(), IntermKind::Suspend, &context);
                    rq.add(RenderData::new(
                        interm.id(),
                        *interm.rect(),
                        UpdateMode::Full,
                    ));
                    tasks::schedule_task(
                        TaskId::PrepareSuspend,
                        Event::PrepareSuspend,
                        PREPARE_SUSPEND_WAIT_DELAY,
                        &kaesar_ev_tx,
                        &mut tasks,
                    );
                    view.children_mut().push(Box::new(interm) as Box<dyn View>);
                }
            }
            _ => {
                handle_event(
                    view.as_mut(),
                    &evt,
                    &kaesar_ev_tx,
                    &mut bus,
                    &mut rq,
                    &mut context,
                );
            }
        }

        process_render_queue(view.as_ref(), &mut rq, &mut context, &mut updating);

        while let Some(ce) = bus.pop_front() {
            kaesar_ev_tx.send(ce).ok();
        }
    }

    if exit_status == ExitStatus::Quit
        && !CURRENT_DEVICE.has_gyroscope()
        && context.display.rotation != initial_rotation
    {
        context.fb.set_rotation(initial_rotation).ok();
    }

    if tasks.iter().all(|task| task.id != TaskId::Suspend) {
        if context.settings.frontlight {
            context.settings.frontlight_levels = context.frontlight.levels();
        }
    }

    context.library.flush();

    let path = Path::new(SETTINGS_PATH);
    save_toml(&context.settings, path).context("can't save settings")?;

    if context.fb.inverted() {
        context.fb.set_inverted(false);
    }

    Ok(match exit_status {
        ExitStatus::Quit => 0,
        ExitStatus::RestartApp => 85,
        ExitStatus::Reboot => 87,
        ExitStatus::PowerOff => 88,
    })
}
