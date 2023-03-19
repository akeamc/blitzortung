//! Internal stream utilities.

use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, ready, FutureExt, Stream, TryStream};

#[cfg(feature = "tracing")]
use tracing::debug;

/// A stream factory.
pub trait Factory {
    /// The stream that this factory produces.
    type Stream: Sized + Unpin + TryStream;
    // this avoids nested Results
    /// Connect error.
    type Error: Into<<Self::Stream as TryStream>::Error>;

    /// Create a new stream.
    fn connect() -> BoxFuture<'static, Result<Self::Stream, Self::Error>>;
}

/// An infinite stream that is guaranteed to never yield `None`.
#[derive(Debug)]
pub struct Infinite<F>
where
    F: Factory,
{
    state: State<F::Stream, F::Error>,
}

impl<F> Infinite<F>
where
    F: Factory,
    <<F as Factory>::Stream as TryStream>::Ok: Debug,
    <<F as Factory>::Stream as TryStream>::Error: Debug,
{
    /// Open a new infinite stream.
    #[must_use]
    pub fn connect() -> Self {
        Self {
            state: State::Connecting(F::connect()),
        }
    }

    /// Force reconnect.
    pub fn reconnect(&mut self) {
        self.state = State::Connecting(F::connect());
    }

    /// Poll for a new item. Unlike [`Stream::poll_next`], this does not return an `Option`.
    #[allow(clippy::type_complexity)] // inherent associated types are unstable
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, ret))]
    pub fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<
        Result<
            <<F as Factory>::Stream as TryStream>::Ok,
            <<F as Factory>::Stream as TryStream>::Error,
        >,
    > {
        let stream = match &mut self.state {
            State::Connected(s) => s,
            State::Connecting(fut) => match ready!(fut.poll_unpin(cx)) {
                Ok(s) => {
                    self.state = State::Connected(s);
                    self.state.stream_mut().unwrap()
                }
                Err(e) => return Poll::Ready(dbg!(Err(e.into()))),
            },
        };

        #[cfg(feature = "tracing")]
        debug!("polling stream");

        ready!(Pin::new(stream).try_poll_next(cx)).map_or_else(
            || {
                // the underlying stream ended
                #[cfg(feature = "tracing")]
                debug!("reconnecting");
                self.reconnect();
                self.poll_next(cx)
            },
            Poll::Ready,
        )
    }
}

impl<F> Stream for Infinite<F>
where
    F: Factory,
    <<F as Factory>::Stream as TryStream>::Ok: Debug,
    <<F as Factory>::Stream as TryStream>::Error: Debug,
{
    type Item = Result<
        <<F as Factory>::Stream as TryStream>::Ok,
        <<F as Factory>::Stream as TryStream>::Error,
    >;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.poll_next(cx).map(Some)
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
