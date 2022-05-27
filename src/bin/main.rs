use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = blitzortung::live::create_stream();

    while let Some(res) = stream.next().await {
        let data = res??;
        println!("{}", data.time);
    }

    Ok(())
}
