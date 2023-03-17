use std::pin::Pin;

use pb::{mc_queen_server::McQueen, StrikeRequest};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status};

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
pub struct MyMcQueen {}

impl MyMcQueen {
    pub fn new() -> Self {
        Self {}
    }
}

type RpcResult<T> = Result<Response<T>, Status>;

#[tonic::async_trait]
impl McQueen for MyMcQueen {
    type StrikesStream = Pin<Box<dyn Stream<Item = Result<pb::Strike, Status>> + Send>>;

    async fn strikes(&self, _req: Request<StrikeRequest>) -> RpcResult<Self::StrikesStream> {
        let mut stream = blitzortung::live::stream();

        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                match tx.send(Result::<_, Status>::Ok(item.unwrap().into())).await {
                    Ok(_) => {
                        // item (server response) was queued to be send to client
                    }
                    Err(_item) => {
                        // output_stream was build from rx and both are dropped
                        break;
                    }
                }
            }
            println!("client disconnected");
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream) as Self::StrikesStream))
    }
}
