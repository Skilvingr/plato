use std::{process::Command, thread, time::Duration};

use crate::{
    context::Context,
    settings::IntermKind,
    view::{intermission::Intermission, render_instantly},
    ExitStatus,
};

pub fn manage_usb(mount: bool, context: &mut Context) -> Option<ExitStatus> {
    let mut ret = None;

    match Command::new(if mount {
        "scripts/usb-enable.sh"
    } else {
        "scripts/usb-disable.sh"
    })
    .status()
    {
        Ok(ret_status) => {
            if !ret_status.success() {
                eprintln!(
                    "Something wrong happened while trying to {} internal storage.",
                    if mount { "share" } else { "unmount" }
                );
                eprintln!("Powering off the device in 3 seconds...");
                ret = Some(ExitStatus::PowerOff);

                let interm = Intermission::new(
                    context.fb.rect(),
                    if mount {
                        IntermKind::CriticalError(
                            "Something wrong happened\nwhile trying to share internal storage.\nPowering off the device\nin 10 seconds..."
                        )
                    } else {
                        IntermKind::CriticalError(
                            "Something wrong happened\nwhile trying to unmount internal storage.\nPowering off the device\nin 10 seconds..."
                        )
                    },
                    context,
                );
                render_instantly(&interm, context);

                thread::sleep(Duration::from_secs(3));
            } else {
                println!(
                    "Internal storage {} correctly.",
                    if mount { "shared" } else { "unmounted" }
                );
            }
        }
        Err(e) => {
            eprintln!(
                "Error {} storage: {e}",
                if mount { "sharing" } else { "unmounting" }
            );
        }
    }

    ret
}
