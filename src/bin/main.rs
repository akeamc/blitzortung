use futures::StreamExt;

#[tokio::main]
async fn main() {
    let mut conn = blitzortung::live::create_stream();
    while let Some(msg) = conn.next().await {
        let msg = msg.unwrap();
        println!("{} ({:.01}s delay)", msg.time, msg.delay.as_seconds_f64());
    }
}
