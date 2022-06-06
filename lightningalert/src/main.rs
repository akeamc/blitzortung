#![warn(clippy::pedantic, clippy::nursery)]

use futures::StreamExt;
use geo::{
    point,
    prelude::{HaversineDistance, VincentyDistance},
    Point,
};
use reverse_geocoder::{Locations, ReverseGeocoder};
use structopt::StructOpt;

use tracing::{debug, info};

fn cc_emoji(cc: &str) -> String {
    cc.chars()
        .filter_map(|c| char::from_u32(c as u32 + 0x1F1A5))
        .collect()
}

#[derive(Debug, StructOpt)]
struct Opt {
    /// Longitude of home.
    #[structopt(long, env = "LAT")]
    lat: f64,
    /// Latitude of home.
    #[structopt(long, env = "LON")]
    lon: f64,
    /// Maximum distance to send alerts (meters).
    #[structopt(long, env = "RADIUS")]
    radius: f64,
}

impl Opt {
    fn home(&self) -> Point<f64> {
        point! { x: self.lon, y: self.lat }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt::init();

    let opt = Opt::from_args();

    // let mut client = Client::builder(&token, GatewayIntents::empty()).await?;

    let loc = Locations::from_memory();
    let geocoder = ReverseGeocoder::new(&loc);

    let mut stream = blitzortung::live::create_stream();

    while let Some(res) = stream.next().await {
        let strike = res?;
        let loc = strike.location();
        let dist = strike
            .location()
            .vincenty_distance(&opt.home())
            .unwrap_or_else(|_| loc.haversine_distance(&opt.home()));

        debug!(
            lat = strike.lat,
            lon = strike.lon,
            dist_km = (dist / 1000.).round(),
            "received strike"
        );

        if dist > opt.radius {
            continue;
        }

        let place = geocoder.search((strike.lat, strike.lon)).unwrap().record;

        info!(
            "{}, {} ({} km)",
            place.name,
            place.cc,
            (dist / 1000.).round()
        );
    }

    unreachable!("a durable stream never ends");
}
