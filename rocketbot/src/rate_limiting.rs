//! Rate limiting logic for WebSocket streams.


use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures_util::{FutureExt, Sink, SinkExt, Stream, StreamExt};
use tokio::net::TcpStream;
use tokio::time::Sleep;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tracing::debug;

use crate::config::RateLimitConfig;


#[derive(Debug)]
pub struct MaybeRateLimitedStream {
    inner: WebSocketStream<MaybeTlsStream<TcpStream>>,
    limit_config: Option<RateLimitConfig>,
    last_instants: Vec<Instant>,
    sleeper: Option<Pin<Box<Sleep>>>,
}
impl MaybeRateLimitedStream {
    pub fn new(inner: WebSocketStream<MaybeTlsStream<TcpStream>>, limit_config: Option<RateLimitConfig>) -> Self {
        Self {
            inner,
            limit_config,
            last_instants: Vec::new(),
            sleeper: None,
        }
    }
}
impl Stream for MaybeRateLimitedStream {
    type Item = Result<Message, WsError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}
impl Sink<Message> for MaybeRateLimitedStream {
    type Error = WsError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        debug!("rate limiter poll: here we go");

        // check if we are already sleeping
        let (have_sleeper, sleeper_elapsed) = if let Some(sleeper) = &mut self.sleeper {
            let slept = sleeper.as_mut().poll(cx);
            (true, slept.is_ready())
        } else {
            (false, false)
        };
        if have_sleeper {
            if sleeper_elapsed {
                self.sleeper = None;
            } else {
                // chillax harder
                debug!("rate limiter poll: not sending: sleeper is still sleeping");
                return Poll::Pending;
            }
        }

        // check the rate limit
        if let Some(limit_config) = self.limit_config {
            assert!(limit_config.max_messages > 0);
            let limit_duration = Duration::from_millis(limit_config.time_slot_ms);
            let now_instant = Instant::now();
            let limit_start_instant = now_instant - limit_duration;
            self.last_instants.retain(|inst| inst >= &limit_start_instant);

            if self.last_instants.len() >= limit_config.max_messages {
                // yup, we need to chillax

                // find when the oldest message falls out
                let oldest_instant = self.last_instants[0];
                let oldest_age_out_instant = oldest_instant + limit_duration;
                assert!(oldest_age_out_instant > now_instant);
                let sleep_length = oldest_age_out_instant - now_instant;

                debug!("rate limiter poll: not sending: we sent {} >= {} messages, so we sleep for {:?}", self.last_instants.len(), limit_config.max_messages, sleep_length);
                let mut sleepy = Box::pin(tokio::time::sleep(sleep_length));

                // poke the sleeper at least once so that it wakes us up when the time comes
                let slept = sleepy.as_mut().poll_unpin(cx);
                if slept.is_ready() {
                    debug!("rate limiter poll: not sleeping after all");
                } else {
                    self.sleeper = Some(sleepy);

                    // time to chillax
                    return Poll::Pending;
                }
            } else {
                // we are out of the rate limit; the stream readiness counts
                debug!("rate limiter poll: possibly sending; few enough messages in window");
            }
        } else {
            // only the stream readiness counts
            debug!("rate limiter poll: possibly sending: no rate limit");
        }

        debug!("rate limiter poll: forwarding to stream");
        self.inner.poll_ready_unpin(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        if self.limit_config.is_some() {
            // sending is governed by the rate limit
            let now = Instant::now();
            self.last_instants.push(now);
        }

        self.inner.start_send_unpin(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.limit_config.is_some() {
            // flushing is governed by the rate limit
            let now = Instant::now();
            self.last_instants.push(now);
        }

        self.inner.poll_flush_unpin(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // we should be able to close at any time
        self.inner.poll_close_unpin(cx)
    }
}
