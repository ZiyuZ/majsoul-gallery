//! Credential-free discovery and route handshake check.
use anyhow::Result;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let policy = majsoul_api::Policy::default();
    let discovery = majsoul_api::discover(&policy).await?;
    println!(
        "Unity {} / resources {} / {}",
        discovery.package_version, discovery.resource_version, discovery.route_id
    );
    let mut client = majsoul_api::Client::connect(discovery, policy).await?;
    client.heartbeat().await?;
    client.close().await;
    println!("Route handshake and heartbeat succeeded; no login was sent.");
    Ok(())
}
