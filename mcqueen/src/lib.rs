use std::{
    pin::Pin,
    sync::{Arc, Mutex},
};

use futures_util::TryStreamExt;
use pb::{mc_queen_server::McQueen, LiveStrikesRequest};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tonic::{Request, Response, Status};
use tracing::debug;

pub mod pb {
    use prost_types::Timestamp;

    tonic::include_proto!("mcqueen");

    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("mcqueen_descriptor");

    impl From<blitzortung::live::Strike> for Strike {
        fn from(value: blitzortung::live::Strike) -> Self {
            let blitzortung::live::Strike {
                time,
                lat,
                lon,
                alt,
                pol: _,
                mds: _,
                mcg: _,
                status: _,
                region: _,
                sig: _,
                delay,
            } = value;

            Self {
                time: Some(Timestamp {
                    seconds: time.unix_timestamp(),
                    nanos: time.nanosecond() as _,
                }),
                location: Some(Location {
                    latitude: lat,
                    longitude: lon,
                }),
                altitude: alt,
                delay: Some(prost_types::Duration {
                    seconds: delay.whole_seconds(),
                    nanos: delay.subsec_nanoseconds(),
                }),
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct MyMcQueen {
    tx: Arc<Mutex<Option<broadcast::Sender<blitzortung::live::Strike>>>>,
}

impl MyMcQueen {
    pub fn new() -> Self {
        Self {
            tx: Arc::new(Mutex::new(None)),
        }
    }
}

type RpcResult<T> = Result<Response<T>, Status>;

#[tonic::async_trait]
impl McQueen for MyMcQueen {
    type LiveStrikesStream = Pin<Box<dyn Stream<Item = Result<pb::Strike, Status>> + Send>>;

    async fn live_strikes(
        &self,
        _req: Request<LiveStrikesRequest>,
    ) -> RpcResult<Self::LiveStrikesStream> {
        let mut stream = blitzortung::live::stream();

        println!("helo");

        let mut tx_mutex = self.tx.lock().unwrap();

        let rx = if let Some(tx) = tx_mutex.as_ref() {
            tx.subscribe()
        } else {
            let (tx, rx) = broadcast::channel(128);
            *tx_mutex = Some(tx.clone());

            let tx_arc = self.tx.clone();
            tokio::spawn(async move {
                while let Some(item) = stream.next().await {
                    match tx.send(item.unwrap()) {
                        Ok(_) => {
                            // item (server response) was queued to be send to client
                        }
                        Err(_e) => {
                            tx_arc.lock().unwrap().take(); // drop tx
                            break;
                        }
                    }
                }
                debug!("clients disconnected");
            });
            rx
        };

        drop(tx_mutex);

        let output_stream = BroadcastStream::new(rx)
            .map_ok(Into::into)
            .map_err(|e| Status::internal(format!("broadcast stream error: {}", e)));
        Ok(Response::new(
            Box::pin(output_stream) as Self::LiveStrikesStream
        ))
    }
}
