#![warn(clippy::pedantic, clippy::nursery)]

use futures::TryStreamExt;
use geo::{
    point,
    prelude::{HaversineDistance, VincentyDistance},
    Point,
};
use reverse_geocoder::{Locations, ReverseGeocoder};
use structopt::StructOpt;

use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

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

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    let opt = Opt::from_args();

    let loc = Locations::from_memory();
    let geocoder = ReverseGeocoder::new(&loc);

    let mut stream = blitzortung::live::stream();

    while let Some(strike) = stream.try_next().await? {
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

    unreachable!("an infinite stream never ends");
}
