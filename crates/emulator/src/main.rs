use kaesar_core::anyhow::{Context as ResultExt, Error};
use kaesar_core::battery::{Battery, FakeBattery};
use kaesar_core::chrono::Local;
use kaesar_core::colour::Colour;
use kaesar_core::context::Context;
use kaesar_core::device::CURRENT_DEVICE;
use kaesar_core::font::Fonts;
use kaesar_core::framebuffer::{Framebuffer, UpdateMode};
use kaesar_core::frontlight::{Frontlight, LightLevels};
use kaesar_core::geom::Rectangle;
use kaesar_core::helpers::load_toml;
use kaesar_core::library::Library;
use kaesar_core::lightsensor::LightSensor;
use kaesar_core::png;
use kaesar_core::settings::Settings;
use sdl2::event::{Event as SdlEvent, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::pixels::{Color as SdlColor, PixelFormatEnum};
use sdl2::rect::Point as SdlPoint;
use sdl2::rect::Rect as SdlRect;
use sdl2::render::{BlendMode, WindowCanvas};
use std::fs::File;
use std::process::exit;
use std::sync::mpsc;
use std::thread;
use std::{env, mem};

use crate::events_sim::{power_btn_up_down, snow};
use crate::unsafe_sync_cell::UnsafeSyncCell;

mod events_sim;
mod unsafe_sync_cell;

pub const APP_NAME: &str = "Kaesar";
const DEFAULT_ROTATION: i8 = 0;

pub fn build_context(fb: Box<dyn Framebuffer>) -> Result<Context, Error> {
    let settings = load_toml::<Settings, _>("emulator_sysroot/Settings.toml")?;
    let library_settings = &settings.libraries[settings.selected_library];
    let library = Library::new(&library_settings.path, library_settings.mode)?;

    let battery = Box::new(FakeBattery::new("emulator_sysroot/battery_sysfs")?) as Box<dyn Battery>;
    let frontlight = Box::new(LightLevels::default()) as Box<dyn Frontlight>;
    let lightsensor = Box::new(0u16) as Box<dyn LightSensor>;
    let fonts = Fonts::load()?;

    Ok(Context::new(
        fb,
        None,
        library,
        settings,
        fonts,
        battery,
        frontlight,
        lightsensor,
    ))
}

struct FBCanvas(Option<WindowCanvas>);

impl Framebuffer for FBCanvas {
    fn set_pixel(&mut self, x: u32, y: u32, color: Colour) {
        let [red, green, blue] = color.rgb();
        self.0
            .as_mut()
            .unwrap()
            .set_draw_color(SdlColor::RGB(red, green, blue));
        self.0
            .as_mut()
            .unwrap()
            .draw_point(SdlPoint::new(x as i32, y as i32))
            .unwrap();
    }

    fn set_blended_pixel(&mut self, x: u32, y: u32, color: Colour, alpha: f32) {
        let [red, green, blue] = color.rgb();
        self.0.as_mut().unwrap().set_draw_color(SdlColor::RGBA(
            red,
            green,
            blue,
            (alpha * 255.0) as u8,
        ));
        self.0
            .as_mut()
            .unwrap()
            .draw_point(SdlPoint::new(x as i32, y as i32))
            .unwrap();
    }

    fn invert_region(&mut self, rect: &Rectangle) {
        let width = rect.width();
        let s_rect = Some(SdlRect::new(rect.min.x, rect.min.y, width, rect.height()));
        if let Ok(data) = self
            .0
            .as_ref()
            .unwrap()
            .read_pixels(s_rect, PixelFormatEnum::RGB24)
        {
            for y in rect.min.y..rect.max.y {
                let v = (y - rect.min.y) as u32;
                for x in rect.min.x..rect.max.x {
                    let u = (x - rect.min.x) as u32;
                    let addr = 3 * (v * width + u);
                    let red = data[addr as usize];
                    let green = data[(addr + 1) as usize];
                    let blue = data[(addr + 2) as usize];
                    let mut color = Colour::Rgb(red, green, blue);
                    color.invert();
                    self.set_pixel(x as u32, y as u32, color);
                }
            }
        }
    }

    fn shift_region(&mut self, rect: &Rectangle, drift: u8) {
        let width = rect.width();
        let s_rect = Some(SdlRect::new(rect.min.x, rect.min.y, width, rect.height()));
        if let Ok(data) = self
            .0
            .as_ref()
            .unwrap()
            .read_pixels(s_rect, PixelFormatEnum::RGB24)
        {
            for y in rect.min.y..rect.max.y {
                let v = (y - rect.min.y) as u32;
                for x in rect.min.x..rect.max.x {
                    let u = (x - rect.min.x) as u32;
                    let addr = 3 * (v * width + u);
                    let red = data[addr as usize];
                    let green = data[(addr + 1) as usize];
                    let blue = data[(addr + 2) as usize];
                    let mut color = Colour::Rgb(red, green, blue);
                    color.shift(drift);
                    self.set_pixel(x as u32, y as u32, color);
                }
            }
        }
    }

    fn update(&mut self, _rect: &Rectangle, _mode: UpdateMode) -> Result<u32, Error> {
        self.0.as_mut().unwrap().present();
        Ok(Local::now().timestamp_subsec_millis())
    }

    fn wait(&self, _tok: u32) -> Result<i32, Error> {
        Ok(1)
    }

    fn save(&self, path: &str) -> Result<(), Error> {
        let (width, height) = self.dims();
        let file =
            File::create(path).with_context(|| format!("can't create output file {}", path))?;
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_color(png::ColorType::Rgb);
        let mut writer = encoder
            .write_header()
            .with_context(|| format!("can't write PNG header for {}", path))?;
        let data = self
            .0
            .as_ref()
            .unwrap()
            .read_pixels(self.0.as_ref().unwrap().viewport(), PixelFormatEnum::RGB24)
            .unwrap_or_default();
        writer
            .write_image_data(&data)
            .with_context(|| format!("can't write PNG data to {}", path))?;
        Ok(())
    }

    fn rotation(&self) -> i8 {
        DEFAULT_ROTATION
    }

    fn set_rotation(&mut self, n: i8) -> Result<(u32, u32), Error> {
        let (mut width, mut height) = self.dims();
        if (width < height && n % 2 != 0) || (width > height && n % 2 != 1) {
            mem::swap(&mut width, &mut height);
        }

        // The canvas here has to be recreated after a resize event:
        // https://wiki.libsdl.org/SDL2/SDL_GetWindowSurface#remarks

        let mut win = self.0.take().unwrap().into_window();
        win.set_size(width, height).unwrap();
        let mut fb = win.into_canvas().software().build().unwrap();
        fb.set_blend_mode(BlendMode::Blend);
        fb.present();
        self.0.replace(fb);

        Ok((width, height))
    }

    fn set_monochrome(&mut self, _enable: bool) {}

    fn set_dithered(&mut self, _enable: bool) {}

    fn set_inverted(&mut self, _enable: bool) {}

    fn monochrome(&self) -> bool {
        false
    }

    fn dithered(&self) -> bool {
        false
    }

    fn inverted(&self) -> bool {
        false
    }

    fn width(&self) -> u32 {
        self.0.as_ref().unwrap().window().size().0
    }

    fn height(&self) -> u32 {
        self.0.as_ref().unwrap().window().size().1
    }
}

fn main() -> Result<(), Error> {
    // Will be searched by the core library in order to set
    // the right device settings
    unsafe {
        env::set_var("PRODUCT", "kaesar_simulator");
    }

    //crate::input::start_input();

    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let (width, height) = CURRENT_DEVICE.dims;
    let window = video_subsystem
        .window("Kaesar Emulator", width, height)
        .position_centered()
        .resizable()
        .build()
        .unwrap();

    let mut fb = window.into_canvas().software().build().unwrap();
    fb.set_blend_mode(BlendMode::Blend);

    let (tx, rx) = mpsc::channel();

    // Little hack to send the SDL context in another thread.
    // Hope nothing will catch fire...
    let sdl_ctx = UnsafeSyncCell::new(sdl_context);
    thread::spawn(move || {
        let ctx = sdl_ctx.inner();

        // File used to mock input events. In an ordinary system would be /dev/input/event*
        let mut evt_file = File::create("emulator_sysroot/sim-touch-evts").unwrap();

        let mut power_key_down = false;
        let mut double_fingers_mod = false;
        loop {
            let mut event_pump = ctx.event_pump().unwrap();
            while let Some(sdl_evt) = event_pump.poll_event() {
                //println!("EVT: {:#?}", sdl_evt);

                match sdl_evt {
                    SdlEvent::Quit { .. } => {
                        exit(0);
                    }
                    SdlEvent::KeyDown {
                        timestamp, keycode, ..
                    } => {
                        if let Some(keycode) = keycode {
                            match keycode {
                                Keycode::LCTRL => {
                                    double_fingers_mod = true;
                                }
                                Keycode::P if !power_key_down => {
                                    power_key_down = true;
                                    power_btn_up_down(&mut evt_file, timestamp, false);
                                }
                                _ => {}
                            }
                        }
                    }

                    SdlEvent::KeyUp {
                        timestamp, keycode, ..
                    } => {
                        if let Some(keycode) = keycode {
                            match keycode {
                                Keycode::LCTRL => {
                                    double_fingers_mod = false;
                                }
                                Keycode::P => {
                                    power_key_down = false;
                                    power_btn_up_down(&mut evt_file, timestamp, true);
                                }
                                _ => {}
                            }
                        }
                    }
                    SdlEvent::MouseMotion {
                        timestamp,
                        mousestate,
                        x,
                        y,
                        ..
                    } if mousestate.is_mouse_button_pressed(MouseButton::Left) => {
                        snow::_finger_move(&mut evt_file, timestamp, x, y, double_fingers_mod);
                    }
                    SdlEvent::MouseButtonDown {
                        timestamp,
                        mouse_btn,
                        x,
                        y,
                        clicks,
                        ..
                    } if mouse_btn == MouseButton::Left => {
                        snow::_finger_up_down(
                            &mut evt_file,
                            timestamp,
                            x,
                            y,
                            false,
                            double_fingers_mod,
                        );
                    }
                    SdlEvent::MouseButtonUp {
                        timestamp,
                        mouse_btn,
                        x,
                        y,
                        clicks,
                        ..
                    } if mouse_btn == MouseButton::Left => {
                        snow::_finger_up_down(
                            &mut evt_file,
                            timestamp,
                            x,
                            y,
                            true,
                            double_fingers_mod,
                        );
                    }
                    SdlEvent::Window { win_event, .. } => {
                        if let WindowEvent::Resized(_, _) = win_event {
                            tx.send(()).unwrap();
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    let context = build_context(Box::new(FBCanvas(Some(fb))))?;
    kaesar::run(context, 0, rx).unwrap();

    Ok(())
}
