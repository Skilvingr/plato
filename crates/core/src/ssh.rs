use std::{fs, process::Command, sync::mpsc::Sender, thread, time::Duration};

use crate::{context::Context, view::Event};

pub fn set_ssh(enable: bool, context: &mut Context, tx: &Sender<Event>) {
    context.settings.ssh = enable;
    let mut pidof = Command::new("pidof");

    if context.settings.ssh {
        let pidof = pidof.arg("dropbear");

        if pidof.status().is_ok_and(|s| s.success()) {
            let _ = tx.send(Event::SshUp("Ssh server already running."));
        } else if fs::exists("../koreader/dropbear").is_ok_and(|x| x) {
            if !fs::exists("/dev/pts").is_ok_and(|x| x) {
                match Command::new("mkdir")
                    .args(["-p", "/dev/pts"])
                    .status()
                    .and_then(|s| {
                        println!("/dev/pts created with {s}.");

                        Command::new("mount")
                            .args(["-t devpts devpts", "/dev/pts"])
                            .status()
                    }) {
                    Ok(s) => println!("/dev/pts created and mounted correctly with {s}."),
                    Err(e) => eprintln!("Error creating or mounting /dev/pts: {e}."),
                }
            }

            if !fs::exists("settings/SSH").is_ok_and(|x| x) {
                match fs::create_dir_all("settings/SSH") {
                    Ok(_) => println!("settings/SSH created correctly."),
                    Err(e) => eprintln!("Error creating settings/SSH: {e}."),
                }
            }

            let tx = tx.clone();
            thread::spawn(move || {
                match Command::new("../koreader/dropbear")
                    .args(["-E", "-R", "-p", "2222", "-n"])
                    .status()
                {
                    Ok(s) if s.success() => {
                        println!("ssh server started with {s}.");
                        let _ = tx.send(Event::SshUp("Ssh server started successfully."));
                    }
                    Ok(_) => {
                        let _ = tx.send(Event::SshUp("Error starting SSH server."));
                    }
                    Err(e) => {
                        eprintln!("Error starting SSH server: {e}.");
                        let _ = tx.send(Event::SshUp("Error starting SSH server."));
                    }
                }
            });
        }
    } else {
        let tx = tx.clone();
        thread::spawn(move || {
            let pidof = pidof.arg("dropbear");

            let _ = Command::new("killall").arg("dropbear").status();

            for _ in 0..20 {
                if !pidof.status().is_ok_and(|s| s.success()) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }

            if pidof.status().is_ok_and(|s| s.success()) {
                let _ = Command::new("killall").args(["-KILL", "dropbear"]).status();

                for _ in 0..20 {
                    if !pidof.status().is_ok_and(|s| s.success()) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }

            if !pidof.status().is_ok_and(|s| s.success()) {
                println!("Dropbear stopped successfully.");
                let _ = tx.send(Event::SshUp("SSH server stopped successfully."));
            } else {
                eprintln!("Failed to stop Dropbear.");
                let _ = tx.send(Event::SshUp("Error stopping SSH server."));
            }
        });
    }
}
