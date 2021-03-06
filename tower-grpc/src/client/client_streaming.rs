use super::streaming;
use codec::Streaming;
use error::Error;
use Body;

use std::fmt;

use futures::{Future, Poll, Stream};
use http::{response, Response};
use prost::Message;

pub struct ResponseFuture<T, U, B: Body> {
    state: State<T, U, B>,
}

enum State<T, U, B: Body> {
    /// Waiting for the HTTP response
    WaitResponse(streaming::ResponseFuture<T, U>),
    /// Waiting for the gRPC Proto message in the Response body
    WaitMessage {
        head: Option<response::Parts>,
        stream: Streaming<T, B>,
    },
}

impl<T, U, B: Body> ResponseFuture<T, U, B> {
    /// Create a new client-streaming response future.
    pub(super) fn new(inner: streaming::ResponseFuture<T, U>) -> Self {
        let state = State::WaitResponse(inner);
        ResponseFuture { state }
    }
}

impl<T, U, B> Future for ResponseFuture<T, U, B>
where
    T: Message + Default,
    U: Future<Item = Response<B>>,
    U::Error: Into<Error>,
    B: Body,
    B::Error: Into<Error>,
{
    type Item = ::Response<T>;
    type Error = ::Status;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let response = match self.state {
                State::WaitResponse(ref mut inner) => try_ready!(inner.poll()),
                State::WaitMessage {
                    ref mut head,
                    ref mut stream,
                } => {
                    let message = match try_ready!(stream.poll()) {
                        Some(message) => message,
                        None => {
                            return Err(::Status::new(
                                ::Code::Internal,
                                "Missing response message.",
                            ));
                        }
                    };

                    let head = head.take().unwrap();
                    let response = Response::from_parts(head, message);

                    return Ok(::Response::from_http(response).into());
                }
            };

            let (head, body) = response.into_http().into_parts();

            self.state = State::WaitMessage {
                head: Some(head),
                stream: body,
            };
        }
    }
}

impl<T, U, B> fmt::Debug for ResponseFuture<T, U, B>
where
    T: fmt::Debug,
    U: fmt::Debug,
    B: Body + fmt::Debug,
    B::Data: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ResponseFuture")
            .field("state", &self.state)
            .finish()
    }
}

impl<T, U, B> fmt::Debug for State<T, U, B>
where
    T: fmt::Debug,
    U: fmt::Debug,
    B: Body + fmt::Debug,
    B::Data: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::WaitResponse(ref future) => f.debug_tuple("WaitResponse").field(future).finish(),
            State::WaitMessage {
                ref head,
                ref stream,
            } => f
                .debug_struct("WaitMessage")
                .field("head", head)
                .field("stream", stream)
                .finish(),
        }
    }
}
