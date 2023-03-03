use anyhow;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::UnboundedSender;
pub use tokio_util::sync::CancellationToken;

use crate::actor::{Actor, Message};
use crate::handler::BoxedMessageHandler;
use crate::system::{ActorSystem, SystemEvent};
use crate::timer::Timer;

pub mod actor;
pub mod handler;
pub mod system;
pub mod bus;
pub mod runner;
pub mod group;
pub mod timer;


#[derive(Debug, Eq, PartialEq)]
pub enum ActorTrySendError<T> {
    /// The data could not be sent on the channel because the channel is
    /// currently full and sending would require blocking.
    Full(T),

    /// The receive half of the channel was explicitly closed or has been
    /// dropped.
    Closed(T),

    /// No response
    Failed,
}

#[derive(Error, Debug)]
pub enum ActorError {
    #[error("Actor queues congested")]
    Congestion,

    #[error("Actor creation failed")]
    CreateError,

    #[error("Sending message failed")]
    SendError,

    #[error("Actor runtime error")]
    RuntimeError(anyhow::Error),
}

impl ActorError {
    pub fn new<E>(error: E) -> Self
        where
            E: std::error::Error + Send + Sync + 'static,
    {
        Self::RuntimeError(anyhow::Error::new(error))
    }
}


/// The actor context gives a running actor access to its path, as well as the system that
/// is running it.
pub struct ActorContext<E: SystemEvent, A: Actor<E> + ?Sized> {
    system: ActorSystem<E>,
    timer_tx: UnboundedSender<BoxedMessageHandler<E, A>>,
}


impl<E: SystemEvent, A: Actor<E>> ActorContext<E, A> {
    pub fn new(system: ActorSystem<E>, timer_tx: UnboundedSender<BoxedMessageHandler<E, A>>) -> ActorContext<E, A> {
        ActorContext { system: system, timer_tx: timer_tx }
    }

    pub fn system<'t>(&'t self) -> &'t ActorSystem<E> {
        &self.system
    }

    pub fn timer(&self) -> Timer<E, A> {
        Timer::new(self.timer_tx.clone())
    }
}


pub type Sender<M> = mpsc::Sender<Box<handler::ActorMessage<M>>>;
pub type Receiver<M> = mpsc::Receiver<Box<handler::ActorMessage<M>>>;

#[derive(Error, Debug, Clone)]
pub struct Gate<M: Message> (Sender<M>);

impl<M: Message> Gate<M> {
    pub fn new(sender: Sender<M>) -> Self { Gate(sender) }

    pub async fn tell(&self, msg: M) -> Result<(), ActorTrySendError<M>> {
        let message = handler::ActorMessage::<M>::new(msg, None);
        if let Err(error) = self.0.try_send(Box::new(message)) {
            log::error!("Failed to tell message! {}", error.to_string());
            match error {
                TrySendError::Closed(b) => Err(ActorTrySendError::Closed(b.get())),
                TrySendError::Full(b) => Err(ActorTrySendError::Full(b.get())),
            }
        } else {
            Ok(())
        }
    }

    pub async fn ask(&self, msg: M) -> Result<M::Response, ActorTrySendError<M>> {
        let (response_sender, response_receiver) = oneshot::channel();
        let message = handler::ActorMessage::<M>::new(msg, Some(response_sender));
        if let Err(error) = self.0.try_send(Box::new(message)) {
            log::error!("Failed to ask message! {}", error.to_string());
            match error {
                TrySendError::Closed(b) => Err(ActorTrySendError::Closed(b.get())),
                TrySendError::Full(b) => Err(ActorTrySendError::Full(b.get())),
            }
        } else {
            response_receiver
                .await
                .map_err(|_error| ActorTrySendError::Failed)
        }
    }
}


#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use crate::actor::{Actor, Handler};
    use crate::bus::EventBus;

    use super::*;

    #[derive(Debug, Clone)]
    enum TestSystemEvent {
        None
    }

    impl SystemEvent for TestSystemEvent {}


    #[derive(Clone, Debug, PartialEq)]
    struct TestActor1 {
        pub pre_count: usize,
        pub event_count: usize,
        pub msg_count: usize,
        pub post_count: usize,

    }

    #[derive(Clone, Debug, PartialEq)]
    struct TestActor2 {
        pub pre_count: usize,
        pub event_count: usize,
        pub msg_count: usize,
        pub post_count: usize,
    }


    #[async_trait]
    impl Actor<TestSystemEvent> for TestActor1 {
        async fn event(&mut self, _event: &TestSystemEvent, _ctx: &mut ActorContext<TestSystemEvent, Self>) -> Result<(), ActorError> {
            self.event_count += 1;
            Ok(())
        }


        /// Override this function if you like to perform initialization of the actor
        async fn pre_start(&mut self, _ctx: &mut ActorContext<TestSystemEvent, Self>) -> Result<(), ActorError> {
            self.pre_count += 1;
            Ok(())
        }

        /// Override this function if you like to perform work when the actor is stopped
        async fn post_stop(&mut self, _ctx: &mut ActorContext<TestSystemEvent, Self>) {
            self.post_count += 1;
        }
    }

    #[async_trait]
    impl Actor<TestSystemEvent> for TestActor2 {
        async fn event(&mut self, _event: &TestSystemEvent, _ctx: &mut ActorContext<TestSystemEvent, Self>) -> Result<(), ActorError> {
            self.event_count += 1;
            Ok(())
        }


        /// Override this function if you like to perform initialization of the actor
        async fn pre_start(&mut self, _ctx: &mut ActorContext<TestSystemEvent, Self>) -> Result<(), ActorError> {
            self.pre_count += 1;
            Ok(())
        }

        /// Override this function if you like to perform work when the actor is stopped
        async fn post_stop(&mut self, _ctx: &mut ActorContext<TestSystemEvent, Self>) {
            self.post_count += 1;
        }
    }

    #[derive(Clone, Debug)]
    struct TestMessage(pub String);

    #[derive(Clone, Debug)]
    enum TestResponse { Ok, Fail }

    impl Message for TestMessage {
        type Response = TestResponse;
    }


    #[async_trait]
    impl Handler<TestSystemEvent, TestMessage> for TestActor1 {
        async fn handle(&mut self, _msg: TestMessage, _ctx: &mut ActorContext<TestSystemEvent, Self>) -> TestResponse {
            self.msg_count += 1;
            TestResponse::Ok
        }
    }

    #[async_trait]
    impl Handler<TestSystemEvent, TestMessage> for TestActor2 {
        async fn handle(&mut self, _msg: TestMessage, _ctx: &mut ActorContext<TestSystemEvent, Self>) -> TestResponse {
            self.msg_count += 1;
            TestResponse::Fail
        }
    }

    #[tokio::test]
    async fn create_system_and_join() {
        let bus = EventBus::new(200);
        let system: ActorSystem<TestSystemEvent> = ActorSystem::new(bus);

        let actor1 = TestActor1 { pre_count: 0, event_count: 0, msg_count: 0, post_count: 0 };
        let group = system.create_group();
        let (join_handle1, _gate1) = group.single_gated_actor(actor1, 200);

        // trigger event
        system.publish(TestSystemEvent::None);

        // gates of same type may be managed in containers
        group.terminate();

        // wait for terminated actor1 and actor2
        let terminated = join_handle1.await;
        assert!(terminated.is_ok());
    }

    #[tokio::test]
    async fn create_system_handle_message() {
        let bus = EventBus::new(200);
        let system: ActorSystem<TestSystemEvent> = ActorSystem::new(bus);

        let actor1 = TestActor1 { pre_count: 0, event_count: 0, msg_count: 0, post_count: 0 };
        let actor2 = TestActor2 { pre_count: 0, event_count: 0, msg_count: 0, post_count: 0 };
        let group = system.create_group();
        let (join_handle1, gate1) = group.single_gated_actor(actor1, 200);
        let (join_handle2, gate2) = group.single_gated_actor(actor2, 200);

        system.publish(TestSystemEvent::None);

        // gates of same type may be managed in containers
        for gate in vec![gate1.clone(), gate2.clone()].iter() {
            let _ = gate.tell(TestMessage("Nachricht 1".to_string())).await;
        }

        for gate in vec![gate1.clone(), gate2.clone()].iter() {
            let _ = gate.tell(TestMessage("Nachricht 2".to_string())).await;
        }

        system.publish(TestSystemEvent::None);

        group.terminate();

        // wait for terminated actor1 and actor2
        let terminated = join_handle1.await;
        assert!(terminated.is_ok());
        assert_eq!(terminated.unwrap(), TestActor1 { pre_count: 1, event_count: 2, msg_count: 2, post_count: 1 });

        let terminated = join_handle2.await;
        assert!(terminated.is_ok());
        assert_eq!(terminated.unwrap(), TestActor2 { pre_count: 1, event_count: 2, msg_count: 2, post_count: 1 });
    }
}
