use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = blitzortung::live::create_stream();

    while let Some(res) = stream.next().await {
        let strike = res?;
        println!("{}", strike.time);
    }

    Ok(())
}
