use std::path::Path;

use kaesar_core::{
    anyhow::{format_err, Context as ResultExt, Error},
    battery::{Battery, KoboBattery},
    context::Context,
    device::{FrontlightKind, CURRENT_DEVICE},
    font::Fonts,
    framebuffer::{Framebuffer, KoboFramebuffer1, KoboFramebuffer2},
    frontlight::{Frontlight, NaturalFrontlight, PremixedFrontlight, StandardFrontlight},
    helpers::load_toml,
    library::Library,
    lightsensor::{KoboLightSensor, LightSensor},
    rtc::Rtc,
    settings::{Settings, SETTINGS_PATH},
};

const RTC_DEVICE: &str = "/dev/rtc0";
const FB_DEVICE: &str = "/dev/fb0";

fn build_context(fb: Box<dyn Framebuffer>) -> Result<Context, Error> {
    let rtc = Rtc::new(RTC_DEVICE)
        .map_err(|e| eprintln!("Can't open RTC device: {:#}.", e))
        .ok();
    let path = Path::new(SETTINGS_PATH);
    let mut settings = if path.exists() {
        load_toml::<Settings, _>(path).context("can't load settings")?
    } else {
        Default::default()
    };

    if settings.libraries.is_empty() {
        return Err(format_err!("no libraries found"));
    }

    if settings.selected_library >= settings.libraries.len() {
        settings.selected_library = 0;
    }

    let library_settings = &settings.libraries[settings.selected_library];
    let library = Library::new(&library_settings.path, library_settings.mode)?;

    let fonts = Fonts::load().context("can't load fonts")?;

    let battery = Box::new(KoboBattery::new().context("can't create battery")?) as Box<dyn Battery>;

    let lightsensor = if CURRENT_DEVICE.has_lightsensor() {
        Box::new(KoboLightSensor::new().context("can't create light sensor")?)
            as Box<dyn LightSensor>
    } else {
        Box::new(0u16) as Box<dyn LightSensor>
    };

    let levels = settings.frontlight_levels;
    let frontlight = match CURRENT_DEVICE.frontlight_kind() {
        FrontlightKind::Standard => Box::new(
            StandardFrontlight::new(levels.intensity)
                .context("can't create standard frontlight")?,
        ) as Box<dyn Frontlight>,
        FrontlightKind::Natural => Box::new(
            NaturalFrontlight::new(levels.intensity, levels.warmth)
                .context("can't create natural frontlight")?,
        ) as Box<dyn Frontlight>,
        FrontlightKind::Premixed => Box::new(
            PremixedFrontlight::new(levels.intensity, levels.warmth)
                .context("can't create premixed frontlight")?,
        ) as Box<dyn Frontlight>,
    };

    Ok(Context::new(
        fb,
        rtc,
        library,
        settings,
        fonts,
        battery,
        frontlight,
        lightsensor,
    ))
}

fn main() -> Result<(), Error> {
    let mut fb: Box<dyn Framebuffer> = if CURRENT_DEVICE.mark() != 8 {
        Box::new(KoboFramebuffer1::new(FB_DEVICE).context("can't create framebuffer")?)
    } else {
        Box::new(KoboFramebuffer2::new(FB_DEVICE).context("can't create framebuffer")?)
    };

    let initial_rotation = CURRENT_DEVICE.transformed_rotation(fb.rotation());
    let startup_rotation = CURRENT_DEVICE.startup_rotation();
    if !CURRENT_DEVICE.has_gyroscope() && initial_rotation != startup_rotation {
        fb.set_rotation(startup_rotation).ok();
    }

    let _context = build_context(fb).context("can't build context")?;

    // Use this cfg only to avoid linting errors when the feature is set in the IDE.
    #[cfg(not(feature = "sim"))]
    std::process::exit(kaesar::run(_context, initial_rotation)?);

    #[cfg(feature = "sim")]
    Ok(())
}
