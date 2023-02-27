use anyhow;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::error::TrySendError;
pub use tokio_util::sync::CancellationToken;

use crate::actor::Message;
use crate::system::{ActorSystem, SystemEvent};

pub mod actor;
pub mod handler;
pub mod system;
pub mod bus;
pub mod runner;
pub mod group;


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
    #[error("Actor creation failed")]
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
pub struct ActorContext<E: SystemEvent> {
    pub system: ActorSystem<E>,
}

// TODO: try to get rid of Box
pub type Sender<M> = mpsc::Sender<Box<handler::ActorMessage<M>>>;
pub type Receiver<M> = mpsc::Receiver<Box<handler::ActorMessage<M>>>;

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


    #[derive(Clone, Debug)]
    struct TestActor1 {
        pub num: usize,
    }

    #[derive(Clone, Debug)]
    struct TestActor2 {
        pub num: usize,
    }


    #[async_trait]
    impl Actor<TestSystemEvent> for TestActor1 {
        async fn event(&mut self, _event: &TestSystemEvent, _ctx: &mut ActorContext<TestSystemEvent>) -> Result<(), ActorError> {
            self.num += 1;
            Ok(())
        }
    }

    #[async_trait]
    impl Actor<TestSystemEvent> for TestActor2 {
        async fn event(&mut self, _event: &TestSystemEvent, _ctx: &mut ActorContext<TestSystemEvent>) -> Result<(), ActorError> {
            self.num += 1;
            Ok(())
        }
    }

    #[derive(Clone, Debug)]
    struct TestMessage (pub String);

    #[derive(Clone, Debug)]
    enum TestResponse { Ok, Fail }

    impl Message for TestMessage {
        type Response = TestResponse;
    }


    #[async_trait]
    impl Handler<TestSystemEvent, TestMessage> for TestActor1 {
        async fn handle(&mut self, _msg: TestMessage, _ctx: &mut ActorContext<TestSystemEvent>) -> TestResponse {
            TestResponse::Fail
        }
    }

    #[async_trait]
    impl Handler<TestSystemEvent, TestMessage> for TestActor2 {
        async fn handle(&mut self, _msg: TestMessage, _ctx: &mut ActorContext<TestSystemEvent>) -> TestResponse {
            TestResponse::Fail
        }
    }

    #[tokio::test]
    async fn create_system_and_join() {
        let bus = EventBus::new(200);
        let system: ActorSystem<TestSystemEvent> = ActorSystem::new(bus);
        system.publish(TestSystemEvent::None);

        let actor1 = TestActor1 { num: 0 };
        let actor2 = TestActor2 { num: 0 };
        let group = system.create_group();
        let (join_handle1, gate1) = group.single_gated_actor(actor1, 200);
        let (join_handle2, gate2) = group.single_gated_actor(actor2, 200);
        // gates of same type may be managed in containers
        for gate in vec![gate1, gate2].iter() {
            gate.tell(TestMessage("Hallo".to_string())).await;
        }
        group.terminate();

        // wait for terminated actor1 and actor2
        let terminated = join_handle1.await;
        assert!(terminated.is_ok());
        let terminated = join_handle2.await;
        assert!(terminated.is_ok());
    }
}
