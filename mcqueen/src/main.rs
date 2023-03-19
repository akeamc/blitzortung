use mcqueen::MyMcQueen;

use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(mcqueen::pb::FILE_DESCRIPTOR_SET)
        .build()
        .unwrap();

    let server = MyMcQueen::new();
    Server::builder()
        .add_service(reflection)
        .add_service(mcqueen::pb::mc_queen_server::McQueenServer::new(server))
        .serve("0.0.0.0:8000".parse().unwrap())
        .await
        .unwrap();

    Ok(())
}
