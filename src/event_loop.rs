use futures::Stream;
use std::fmt;
use std::thread::{self, JoinHandle};
use tokio::runtime::current_thread;
use tokio::sync::mpsc::{self, UnboundedSender};

/// Post messages to event loop
pub fn post<F>(tx: &mut UnboundedSender<EventLoopMsg>, f: F)
where
    F: FnOnce() + Send + 'static,
{
    let msg = EventLoopMsg::new(f);
    if let Err(e) = tx.try_send(msg) {
        println!("Error posting messages to event loop: {:?}", e);
    }
}

/// Message that event loop can accept in order to be requested to do something
pub struct EventLoopMsg(Option<Box<FnMut() + Send>>);

impl EventLoopMsg {
    /// Create a new message to be posted to the event loop
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        let mut f = Some(f);
        EventLoopMsg(Some(Box::new(move || {
            let f = unwrap!(f.take());
            f()
        })))
    }

    /// Create a terminator message which when posted is going to try and exit the event loop. This
    /// is for graceful termination
    pub fn terminator() -> Self {
        Self(None)
    }
}

impl fmt::Debug for EventLoopMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "EventLoopMsg with {} inner",
            if self.0.is_some() { "some" } else { "no" }
        )
    }
}

pub struct EventLoop {
    tx: UnboundedSender<EventLoopMsg>,
    j: Option<JoinHandle<()>>,
}

impl EventLoop {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<EventLoopMsg>();

        let j = unwrap!(thread::Builder::new()
            .name("Crust-Event-Loop".into())
            .spawn(move || {
                let event_loop_future = rx.map_err(|_| ()).for_each(move |ev_loop_msg| {
                    if let Some(mut f) = ev_loop_msg.0 {
                        f();
                        Ok(())
                    } else {
                        Err(())
                    }
                });

                // let mut rt = unwrap!(tokio::runtime::current_thread::Runtime::new());
                // let _ = rt.block_on(event_loop_future);
                current_thread::run(event_loop_future);
                println!("Exiting Crust Event Loop");
            }));

        Self { tx, j: Some(j) }
    }

    #[allow(unused)]
    pub fn tx(&mut self) -> &mut UnboundedSender<EventLoopMsg> {
        &mut self.tx
    }

    /// Post messages to event loop
    pub fn post<F>(&mut self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        post(&mut self.tx, f)
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        if let Err(e) = self.tx.try_send(EventLoopMsg::terminator()) {
            println!("Error trying to send an event loop terminator: {:?}", e);
        }
        let j = unwrap!(self.j.take());
        if let Err(e) = j.join() {
            println!("Error joining the event loop thread: {:?}", e);
        }
    }
}