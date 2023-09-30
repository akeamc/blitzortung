use std::{
    pin::Pin,
    sync::{Arc, Mutex},
};

use futures_util::{StreamExt, TryStreamExt};
use pb::{mc_queen_server::McQueen, LiveStrikesRequest};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, Stream};
use tonic::{Request, Response, Status};
use tracing::{debug, error};

pub mod pb {
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
                region,
                sig,
                delay,
            } = value;

            Self {
                time: Some(prost_types::Timestamp {
                    seconds: time.unix_timestamp(),
                    nanos: time.nanosecond() as _,
                }),
                delay: Some(prost_types::Duration {
                    seconds: delay.whole_seconds(),
                    nanos: delay.subsec_nanoseconds(),
                }),
                location: Some(Location {
                    latitude: lat,
                    longitude: lon,
                }),
                altitude: alt,
                region: region as _,
                stations: sig.into_iter().map(Into::into).collect(),
            }
        }
    }

    impl From<blitzortung::live::Station> for Station {
        fn from(value: blitzortung::live::Station) -> Self {
            let blitzortung::live::Station {
                sta,
                time,
                lat,
                lon,
                alt,
                status,
            } = value;

            Self {
                id: sta,
                time: Some(prost_types::Duration {
                    seconds: time.whole_seconds(),
                    nanos: time.subsec_nanoseconds(),
                }),
                location: Some(Location {
                    latitude: lat,
                    longitude: lon,
                }),
                altitude: alt,
                status,
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

    fn receiver(&self) -> broadcast::Receiver<blitzortung::live::Strike> {
        let mut tx_mutex = self.tx.lock().unwrap();

        if let Some(tx) = tx_mutex.as_ref() {
            tx.subscribe()
        } else {
            let (tx, rx) = broadcast::channel(128);
            *tx_mutex = Some(tx.clone());

            let tx_arc = self.tx.clone();
            tokio::spawn(async move {
                let mut stream = blitzortung::live::stream();

                while let Some(res) = stream.next().await {
                    match res {
                        Ok(item) => match tx.send(item) {
                            Ok(rxs) => debug!(%rxs, "broadcasted strike"),
                            Err(_e) => {
                                // all clients disconnected
                                tx_arc.lock().unwrap().take(); // drop tx
                                debug!("all clients disconnected");
                                break;
                            }
                        },
                        Err(e) => error!("stream error: {e}"),
                    };
                }
            });

            rx
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
        Ok(Response::new(
            BroadcastStream::new(self.receiver())
                .map_ok(Into::into)
                .map_err(|e| Status::internal(format!("broadcast stream error: {}", e)))
                .boxed(),
        ))
    }
}
