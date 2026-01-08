use std::process::Command;

use crate::context::Context;

pub fn set_wifi_tmp(enable: bool) {
    if enable {
        Command::new("scripts/wifi-enable.sh").spawn().ok();
    } else {
        Command::new("scripts/wifi-disable.sh").status().ok();
    }
}

pub fn set_wifi_perm(enable: bool, context: &mut Context) {
    if context.settings.wifi == enable {
        return;
    }

    context.settings.wifi = enable;
    set_wifi_tmp(enable);

    if !context.settings.wifi {
        context.online = false;
    }
}
