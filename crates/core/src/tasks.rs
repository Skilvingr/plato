use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use crate::view::Event;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TaskId {
    PrepareSuspend,
    Suspend,
}

pub struct Task {
    pub id: TaskId,
    _chan: Receiver<()>,
}

pub fn schedule_task(
    id: TaskId,
    event: Event,
    delay: Duration,
    hub: &Sender<Event>,
    tasks: &mut Vec<Task>,
) {
    let (ty, ry) = mpsc::channel();
    let hub2 = hub.clone();
    tasks.retain(|task| task.id != id);
    tasks.push(Task { id, _chan: ry });
    thread::spawn(move || {
        thread::sleep(delay);
        if ty.send(()).is_ok() {
            hub2.send(event).ok();
        }
    });
}
