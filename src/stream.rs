use std::{
    error::Error,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{ready, Future, FutureExt, Stream};

pub enum DisposableResult<T> {
    Some(T),
    None,
    /// Discard the current stream.
    Discard,
}

/// A stream that can be aborted in order to be reconstructed
/// later.
#[allow(clippy::module_name_repetitions)]
pub trait DisposableStream {
    type Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<DisposableResult<Self::Item>>;
}

type BoxedFuture<T> = Pin<Box<dyn Future<Output = T>>>;

#[allow(clippy::module_name_repetitions)]
pub trait CreateStream {
    type Stream: Sized + Unpin + DisposableStream;
    type Error: Error;

    fn connect() -> BoxedFuture<Result<Self::Stream, Self::Error>>;
}

/// A tough stream that automatically reconnects on
/// unexpected disconnects.
#[allow(clippy::module_name_repetitions)]
pub struct DurableStream<T>
where
    T: CreateStream,
{
    state: State<T::Stream, T::Error>,
}

impl<T> DurableStream<T>
where
    T: CreateStream,
{
    pub fn connect() -> Self {
        Self {
            state: State::Connecting(T::connect()),
        }
    }

    pub fn reconnect(&mut self) {
        println!("reconnecting!!!!!");
        self.state = State::Connecting(T::connect());
    }
}

impl<T> Stream for DurableStream<T>
where
    T: CreateStream,
{
    type Item = <<T as CreateStream>::Stream as DisposableStream>::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let stream = match &mut self.state {
            State::Connected(s) => s,
            State::Connecting(fut) => match ready!(fut.poll_unpin(cx)) {
                Ok(s) => {
                    self.state = State::Connected(s);
                    self.state.stream_mut().unwrap()
                }
                Err(_e) => todo!(),
            },
        };

        match ready!(Pin::new(stream).poll_next(cx)) {
            DisposableResult::Some(value) => Poll::Ready(Some(value)),
            DisposableResult::None => Poll::Ready(None),
            DisposableResult::Discard => {
                self.reconnect();
                self.poll_next(cx)
            }
        }
    }
}

enum State<S, E> {
    Connected(S),
    Connecting(Pin<Box<dyn Future<Output = Result<S, E>>>>),
}

impl<S, E> State<S, E> {
    fn stream_mut(&mut self) -> Option<&mut S> {
        match self {
            State::Connected(s) => Some(s),
            State::Connecting(_) => None,
        }
    }
}
