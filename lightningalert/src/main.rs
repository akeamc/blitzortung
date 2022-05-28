use futures::StreamExt;
use geo::{
    point,
    prelude::{HaversineDistance, VincentyDistance},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = blitzortung::live::create_stream();
    let home = point! { x: 0., y: 0. };

    while let Some(res) = stream.next().await {
        let loc = res?.location();
        let dist = loc
            .vincenty_distance(&home)
            .unwrap_or_else(|_| loc.haversine_distance(&home));
        println!("{} m", dist.round());
    }

    Ok(())
}
