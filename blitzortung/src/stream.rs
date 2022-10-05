use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, ready, FutureExt, Stream};

#[derive(Debug)]
pub enum DisposableResult<T> {
    Some(T),
    None,
    /// Discard the current stream and open a new one.
    Discard,
}

impl<T, E> DisposableResult<Result<T, E>> {
    #[must_use]
    pub const fn ok(value: T) -> Self {
        Self::Some(Ok(value))
    }

    #[must_use]
    pub const fn err(err: E) -> Self {
        Self::Some(Err(err))
    }
}

/// Sort of like `TryStream`, since connection errors must
/// be propagated to the stream consumer.
#[allow(clippy::module_name_repetitions)]
pub trait DisposableStream {
    type ItemOk;
    type ItemError;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<DisposableResult<Result<Self::ItemOk, Self::ItemError>>>;
}

#[allow(clippy::module_name_repetitions)]
pub trait CreateStream {
    type Stream: Sized + Unpin + DisposableStream;
    // this avoids nested Results
    type ConnectError: Into<<Self::Stream as DisposableStream>::ItemError>;

    fn connect() -> BoxFuture<'static, Result<Self::Stream, Self::ConnectError>>;
}

/// A tough stream made up of multiple [`DisposableStream`]s,
/// seamlessly moving from one to another at the end of their
/// respective lifetime.
#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct DurableStream<T>
where
    T: CreateStream,
{
    state: State<T::Stream, T::ConnectError>,
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
        self.state = State::Connecting(T::connect());
    }
}

impl<T> Stream for DurableStream<T>
where
    T: CreateStream,
{
    type Item = Result<
        <<T as CreateStream>::Stream as DisposableStream>::ItemOk,
        <<T as CreateStream>::Stream as DisposableStream>::ItemError,
    >;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let stream = match &mut self.state {
            State::Connected(s) => s,
            State::Connecting(fut) => match ready!(fut.poll_unpin(cx)) {
                Ok(s) => {
                    self.state = State::Connected(s);
                    self.state.stream_mut().unwrap()
                }
                Err(e) => return Poll::Ready(Some(Err(e.into()))),
            },
        };

        match ready!(Pin::new(stream).poll_next(cx)) {
            DisposableResult::Some(result) => Poll::Ready(Some(result)),
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
    Connecting(BoxFuture<'static, Result<S, E>>),
}

impl<S, E> State<S, E> {
    fn stream_mut(&mut self) -> Option<&mut S> {
        match self {
            Self::Connected(s) => Some(s),
            Self::Connecting(_) => None,
        }
    }
}

impl<S, E> Debug for State<S, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected(_) => f.debug_tuple("Connected").finish(),
            Self::Connecting(_) => f.debug_tuple("Connecting").finish(),
        }
    }
}
