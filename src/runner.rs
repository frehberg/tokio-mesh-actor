use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender};
pub use tokio_util::sync::CancellationToken;

use crate::{ActorContext, Gate};
use crate::actor::{Actor, Handler, Message};
use crate::bus::EventReceiver;
use crate::handler::{ActorMessage, BoxedMessageHandler};
use crate::handler::MessageHandler;
use crate::system::{ActorSystem, SystemEvent};

pub(crate) struct ActorRunner<E: SystemEvent, A: Actor<E>, M: Message> {
    actor: A,
    cancel_token: CancellationToken,
    event_receiver: EventReceiver<E>,
    receiver: Receiver<Box<ActorMessage<M>>>,
}


impl<E: SystemEvent, A, M: Message> ActorRunner<E, A, M>
    where A: Actor<E> + Handler<E, M> {
    pub fn create(actor: A,
                  event_receiver: EventReceiver<E>,
                  cancel_token: CancellationToken,
                  buffer: usize) -> (Self, Gate<M>) {
        let (sender, receiver)
            : (crate::Sender<M>, crate::Receiver<M>) =
            mpsc::channel(buffer);

        let runner = ActorRunner {
            actor,
            cancel_token,
            event_receiver,
            receiver,
        };
        let gate = Gate::new(sender);
        (runner, gate)
    }


    pub async fn start(mut self, system: ActorSystem<E>) -> A {
        log::debug!("Starting actor '{:p}'...", &self);
        let (timer_tx, mut timer_rx): (UnboundedSender<BoxedMessageHandler<E, A>>, UnboundedReceiver<BoxedMessageHandler<E, A>>) =
            mpsc::unbounded_channel();

        let mut ctx = ActorContext::new(system.clone(), timer_tx);

        // Start the actor
        let start_error = self.actor.pre_start(&mut ctx).await.err();

        // Did we encounter an error at startup? If yes, initiate supervision strategy
        if start_error.is_some() {
            log::error!("Actor '{:p}' failed to start!", &self);
        }
        let mut recv_active = true;
        let mut event_recv_active = true;

        // No errors encountered at startup, so let's run the actor...
        if start_error.is_none() {
            log::debug!("Actor '{:p}' has started successfully.", &self);
            loop {
                tokio::select! {
                    _ = self.cancel_token.cancelled() => {
                       log::debug!("Actor '{:p}' cancelled", &self);
                       break;
                    }
                    // timer event stream never closed
                    Some(mut msg) = timer_rx.recv() => {
                        log::debug!("Actor '{:p}' timer event", &self);
                        msg.handle(&mut self.actor, &mut ctx).await;
                    }
                    // system event stream never closed
                    event = self.event_receiver.recv(), if event_recv_active => {
                        match event {
                            Ok(ev) =>  if let Err(_error) = self.actor.event(&ev, &mut ctx).await {
                                log::debug!("Actor '{:p}' failed event handling", &self);
                            },
                            _ => {
                                log::debug!("Actor '{:p}' system event bus closed unexpectedly", &self);
                                event_recv_active = false;
                            }
                        }
                    }
                    // message stream may be closed if laster sender vanishes
                    recv_mesg = self.receiver.recv(), if recv_active  => {
                        match recv_mesg {
                            Some(mut msg) => {
                                msg.handle(&mut self.actor, &mut ctx).await;
                            }
                            _ => {
                                // receiver cannot receive any message anymore - no more sender
                                log::debug!("Actor '{:p}' receiver has been closed", &self);
                                recv_active = false;
                            }
                        }
                    }
                }
            }

            let mut num_events =
                if event_recv_active { self.event_receiver.len() } else { 0 };

            //
            // terminating - read all remaining messages - ignore broadcast events
            //
            log::debug!("Actor '{:p}' stopping - draining queue", &self);

            // closing message queues, process pending messages
            timer_rx.close();
            self.receiver.close();

            loop {
                tokio::select! {
                    Ok(event) = self.event_receiver.recv(), if num_events > 0 => {
                        log::debug!("Actor '{:p}' stopping - draining event queue", &self);
                        num_events -= 1;
                        if let Err(_error) = self.actor.event(&event, &mut ctx).await {
                            log::debug!("Actor '{:p}' failed event handling", &self);
                        }
                    }
                    // timer event stream never closed
                    Some(mut msg) = timer_rx.recv() => {
                        log::debug!("Actor '{:p}' timer event", &self);
                        msg.handle(&mut self.actor, &mut ctx).await;
                    }
                    recv_mesg = self.receiver.recv(), if recv_active  => {
                        log::debug!("Actor '{:p}' stopping - draining receive queue", &self);
                        match recv_mesg {
                            Some(mut msg) => {
                                msg.handle(&mut self.actor, &mut ctx).await;
                            }
                            _ => {
                                // receiver cannot receive any message anymore - no more sender
                                log::debug!("Actor '{:p}' stopping - receive queue ended", &self);
                                recv_active = false;
                            }
                        }
                    }
                    else => {
                        log::debug!("Actor '{:p}' stopping - receive queue drained", &self);
                        break;
                    }
                }
            }

            log::debug!("Actor '{:p}' stopping - invoking post_stop", &self);

            // Actor receiver has closed, so stop the actor
            self.actor.post_stop(&mut ctx).await;

            log::debug!("Actor '{:p}' stopped.", &self);
        }

        self.actor
    }
}