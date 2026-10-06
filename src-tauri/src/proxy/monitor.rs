use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};

const INTERVAL: u64 = 5_000;
const CAPACITY: usize = 721;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Default, Serialize)]
pub struct Counters {
    pub connections: u64,
    pub accepted_connections: u64,
    pub peak_connections: u64,
    pub active_requests: u64,
    pub peak_requests: u64,
    pub requests: u64,
    pub completed: u64,
    pub errors: u64,
    pub received_bytes: u64,
    pub sent_bytes: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Default, Serialize)]
pub struct Sample {
    pub timestamp: u64,
    pub connections: u64,
    pub active_requests: u64,
    pub requests: u64,
    pub errors: u64,
    pub received_bytes: u64,
    pub sent_bytes: u64,
}

#[derive(Clone, Default, Serialize)]
pub struct RouteStats {
    pub client: String,
    pub protocol: String,
    pub requests: u64,
    pub active_requests: u64,
    pub completed: u64,
    pub errors: u64,
    pub received_bytes: u64,
    pub sent_bytes: u64,
    pub duration_ms: u64,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub since: u64,
    pub sampled_at: u64,
    pub totals: Counters,
    pub rate_seconds: f64,
    pub received_per_second: f64,
    pub sent_per_second: f64,
    pub requests_per_second: f64,
    pub samples: Vec<Sample>,
    pub routes: Vec<RouteStats>,
}

struct Inner {
    since: u64,
    totals: Counters,
    samples: VecDeque<Sample>,
    routes: BTreeMap<String, RouteStats>,
}

#[derive(Clone)]
pub struct Monitor(Arc<Mutex<Inner>>);

impl Default for Monitor {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Inner {
            since: now(),
            totals: Counters::default(),
            samples: VecDeque::new(),
            routes: BTreeMap::new(),
        })))
    }
}

impl Inner {
    fn advance(&mut self, time: u64) {
        let time = time / INTERVAL * INTERVAL;
        if self.samples.is_empty() {
            self.samples.push_back(Sample {
                timestamp: time,
                ..Sample::default()
            });
        }
        let previous = self.samples.back().unwrap().timestamp;
        let start = previous
            .saturating_add(INTERVAL)
            .max(time.saturating_sub((CAPACITY as u64 - 1) * INTERVAL));
        for timestamp in (start..=time).step_by(INTERVAL as usize) {
            if self.samples.len() == CAPACITY {
                self.samples.pop_front();
            }
            self.samples.push_back(Sample {
                timestamp,
                connections: self.totals.connections,
                active_requests: self.totals.active_requests,
                ..Sample::default()
            });
        }
    }
}

impl Monitor {
    fn update(&self, action: impl FnOnce(&mut Inner)) {
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        inner.advance(now());
        action(&mut inner);
        let connections = inner.totals.connections;
        let active = inner.totals.active_requests;
        let last = inner.samples.back_mut().unwrap();
        last.connections = connections;
        last.active_requests = active;
    }
    pub fn totals(&self) -> Counters {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .totals
            .clone()
    }
    fn connection(&self, opened: bool) {
        self.update(|i| {
            if opened {
                i.totals.connections += 1;
                i.totals.accepted_connections += 1;
                i.totals.peak_connections = i.totals.peak_connections.max(i.totals.connections);
            } else {
                i.totals.connections = i.totals.connections.saturating_sub(1);
            }
        });
    }
    fn traffic(&self, received: usize, sent: usize) {
        if received + sent == 0 {
            return;
        }
        self.update(|i| {
            i.totals.received_bytes += received as u64;
            i.totals.sent_bytes += sent as u64;
            let sample = i.samples.back_mut().unwrap();
            sample.received_bytes += received as u64;
            sample.sent_bytes += sent as u64;
        });
    }
    pub fn begin(&self, path: &str) -> String {
        let parts: Vec<_> = path.split('/').filter(|p| !p.is_empty()).collect();
        let client = if parts.first() == Some(&"clients") {
            match parts.get(1).copied().unwrap_or("") {
                "codex" => "codex",
                "claude-code" => "claude-code",
                "opencode" => "opencode",
                "pi" => "pi",
                "deepseek-harness" => "deepseek-harness",
                _ => "unknown",
            }
        } else {
            "api"
        };
        let protocol = match parts.last().copied().unwrap_or("") {
            "responses" => "responses",
            "completions" => "openai_chat",
            "messages" => "anthropic",
            "models" => "models",
            _ => "other",
        };
        let key = format!("{client}:{protocol}");
        self.update(|i| {
            i.totals.requests += 1;
            i.totals.active_requests += 1;
            i.totals.peak_requests = i.totals.peak_requests.max(i.totals.active_requests);
            i.samples.back_mut().unwrap().requests += 1;
            let route = i.routes.entry(key.clone()).or_insert_with(|| RouteStats {
                client: client.into(),
                protocol: protocol.into(),
                ..RouteStats::default()
            });
            route.requests += 1;
            route.active_requests += 1;
        });
        key
    }
    pub fn payload(&self, key: &str, received: usize, sent: usize) {
        self.update(|i| {
            if let Some(route) = i.routes.get_mut(key) {
                route.received_bytes += received as u64;
                route.sent_bytes += sent as u64;
            }
        });
    }
    pub fn finish(&self, key: &str, error: bool, duration: u64) {
        self.update(|i| {
            i.totals.active_requests = i.totals.active_requests.saturating_sub(1);
            i.totals.completed += 1;
            i.totals.errors += u64::from(error);
            i.totals.duration_ms += duration;
            i.samples.back_mut().unwrap().errors += u64::from(error);
            if let Some(route) = i.routes.get_mut(key) {
                route.active_requests = route.active_requests.saturating_sub(1);
                route.completed += 1;
                route.errors += u64::from(error);
                route.duration_ms += duration;
            }
        });
    }
    pub fn snapshot(&self, minutes: u32) -> Snapshot {
        let time = now();
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        inner.advance(time);
        // Rates use complete five-second buckets, excluding the current partial bucket.
        let boundary = time / INTERVAL * INTERVAL;
        let start = boundary.saturating_sub(10_000).max(inner.since);
        let seconds = (boundary.saturating_sub(start)) as f64 / 1000.0;
        let rate: Vec<_> = inner
            .samples
            .iter()
            .filter(|s| s.timestamp >= start / INTERVAL * INTERVAL && s.timestamp < boundary)
            .collect();
        let divisor = seconds.max(1.0);
        Snapshot {
            since: inner.since,
            sampled_at: time,
            totals: inner.totals.clone(),
            rate_seconds: seconds,
            received_per_second: rate.iter().map(|s| s.received_bytes).sum::<u64>() as f64
                / divisor,
            sent_per_second: rate.iter().map(|s| s.sent_bytes).sum::<u64>() as f64 / divisor,
            requests_per_second: rate.iter().map(|s| s.requests).sum::<u64>() as f64 / divisor,
            samples: inner
                .samples
                .iter()
                .filter(|s| {
                    s.timestamp >= time.saturating_sub(minutes.clamp(1, 60) as u64 * 60_000)
                })
                .cloned()
                .collect(),
            routes: inner.routes.values().cloned().collect(),
        }
    }
}

pub struct TrackedListener {
    pub listener: TcpListener,
    pub monitor: Monitor,
}
pub struct TrackedStream {
    stream: TcpStream,
    monitor: Monitor,
}
impl axum::serve::Listener for TrackedListener {
    type Io = TrackedStream;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    self.monitor.connection(true);
                    return (
                        TrackedStream {
                            stream,
                            monitor: self.monitor.clone(),
                        },
                        addr,
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, "TCP accept failed");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }
    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}
impl Drop for TrackedStream {
    fn drop(&mut self) {
        self.monitor.connection(false);
    }
}
impl AsyncRead for TrackedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.stream).poll_read(cx, buf);
        self.monitor.traffic(buf.filled().len() - before, 0);
        result
    }
}
impl AsyncWrite for TrackedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.stream).poll_write(cx, buf);
        if let Poll::Ready(Ok(count)) = &result {
            self.monitor.traffic(0, *count);
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_carries_gauges_and_is_bounded() {
        let mut inner = Inner {
            since: 0,
            totals: Counters {
                connections: 2,
                active_requests: 1,
                ..Counters::default()
            },
            samples: VecDeque::new(),
            routes: BTreeMap::new(),
        };
        inner.advance(0);
        inner.advance(7_200_000);
        assert_eq!(inner.samples.len(), CAPACITY);
        assert_eq!(inner.samples.back().unwrap().connections, 2);
        assert_eq!(inner.samples.back().unwrap().active_requests, 1);
    }
    #[test]
    fn requests_include_errors_and_payload_separately() {
        let monitor = Monitor::default();
        let key = monitor.begin("/clients/codex/v1/responses");
        monitor.payload(&key, 12, 34);
        monitor.finish(&key, true, 50);
        let snapshot = monitor.snapshot(5);
        assert_eq!(snapshot.totals.active_requests, 0);
        assert_eq!(snapshot.totals.errors, 1);
        assert_eq!(snapshot.totals.received_bytes, 0);
        assert_eq!(snapshot.routes[0].sent_bytes, 34);
    }
}
