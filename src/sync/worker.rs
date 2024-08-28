use std::{
    sync::{atomic::Ordering, mpsc, Arc, Mutex},
    thread,
};

use log::info;

use crate::{error::Error, result::Result};

use super::THREAD_SEQ;

pub trait FnBox {
    fn run_task(self: Box<Self>);
}

impl<F: FnOnce()> FnBox for F {
    fn run_task(self: Box<Self>) {
        (*self)()
    }
}

pub type Task = Box<dyn FnBox + Send + 'static>;

pub enum Message {
    NewTask(Task),
    Terminate,
}

pub struct Worker {
    pub id: usize,
    thread: Option<thread::JoinHandle<()>>,
    sender: Option<mpsc::Sender<Message>>,
}

impl Default for Worker {
    fn default() -> Self {
        let (worker_id, sender, thread) = Self::worker_prep(None);
        Self {
            id: worker_id,
            thread: Some(thread),
            sender: Some(sender),
        }
    }
}

impl Worker {
    pub fn new(name: String) -> Self {
        let (worker_id, sender, thread) = Self::worker_prep(Some(name.clone()));
        Self {
            id: worker_id,
            sender: Some(sender),
            thread: Some(thread),
        }
    }

    pub fn new_with_receiver(receiver: Arc<Mutex<mpsc::Receiver<Message>>>) -> Self {
        let worker_id = THREAD_SEQ.fetch_add(1, Ordering::SeqCst);
        let thread = thread::spawn(move || loop {
            let message = receiver.lock().unwrap().recv().unwrap();
            match message {
                Message::NewTask(task) => {
                    task.run_task();
                }
                Message::Terminate => {
                    break;
                }
            }
        });
        Self {
            id: worker_id,
            thread: Some(thread),
            sender: None,
        }
    }

    pub fn send<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce() + Send + 'static,
    {
        let Some(sender) = &self.sender else {
            return Err(Error::UnknownError(Box::from(format!(
                "Worker {}: doesn't have sender",
                self.id
            ))));
        };
        sender
            .send(Message::NewTask(Box::new(f)))
            .map_err(|err| Error::SyncError(Box::new(err)))
    }

    fn worker_prep(name: Option<String>) -> (usize, mpsc::Sender<Message>, thread::JoinHandle<()>) {
        let worker_id = THREAD_SEQ.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel();
        let name = name.unwrap_or(format!("Worker {}", worker_id));
        let thread = thread::Builder::new()
            .name(name.clone())
            .spawn(move || loop {
                let message = receiver.recv().unwrap();
                match message {
                    Message::NewTask(task) => {
                        // info!("Worker {}: received a task", worker_id);
                        task.run_task();
                    }
                    Message::Terminate => {
                        // info!("Worker {}: received termination request", worker_id);
                        break;
                    }
                }
            })
            .unwrap_or_else(|err| panic!("Failed to spawn worker thread: {} with {}", name, err));

        (worker_id, sender, thread)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        info!("Sending terminate message to worker {}", self.id);
        if let Some(sender) = &self.sender {
            sender.send(Message::Terminate).unwrap();
        }
        info!("Shutting down worker {}", self.id);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap()
        }
    }
}
