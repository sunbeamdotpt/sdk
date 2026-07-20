use std::io::Write;

use bollard::{Docker, query_parameters::InspectContainerOptions};
use flate2::{Compression, write::GzEncoder};
use testcontainers::{ContainerAsync, GenericImage, core::ContainerPort};

fn docker_socket() -> String {
    std::env::var("DOCKER_HOST")
        .ok()
        .and_then(|h| h.strip_prefix("unix://").map(|s| s.to_string()))
        .unwrap_or_else(|| "/var/run/docker.sock".to_string())
}

/// Build a small Docker image from an in-memory Dockerfile and extra context files.
///
/// `files` is a list of `(path_in_context, bytes)`.
///
/// This uses `curl` directly against the Docker socket because the BuildKit path in
/// bollard/testcontainers produces a context that socktainer cannot see. `curl` with a
/// classic (`version=1`) build works reliably.
pub async fn build_image(
    descriptor: &str,
    dockerfile: &str,
    files: &[(&str, &[u8])],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut entries: Vec<(&str, &[u8])> = vec![("Dockerfile", dockerfile.as_bytes())];
    entries.extend_from_slice(files);

    for (path, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_path(path)?;
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, data)?;
    }

    let tar_bytes = builder.into_inner()?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_bytes)?;
    let gz_bytes = encoder.finish()?;

    let socket = docker_socket();
    let tar_path = format!(
        "/tmp/sunbeam-test-build-{}.tar",
        descriptor.replace(['/', ':'], "_")
    );
    std::fs::write(&tar_path, &gz_bytes)?;

    let output = std::process::Command::new("curl")
        .arg("-s")
        .arg("--unix-socket")
        .arg(&socket)
        .arg("-X")
        .arg("POST")
        .arg(format!(
            "http://localhost/build?t={descriptor}&dockerfile=Dockerfile&version=1"
        ))
        .arg("-H")
        .arg("Content-Type: application/x-tar")
        .arg("--data-binary")
        .arg(format!("@{tar_path}"))
        .output()?;

    let _ = std::fs::remove_file(&tar_path);

    if !output.status.success() {
        return Err(format!(
            "curl build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
            && (value.get("error").is_some() || value.get("errorDetail").is_some())
        {
            return Err(format!("Docker build error: {line}").into());
        }
    }

    Ok(())
}

/// Return the first bridge IP address of a running container.
///
/// This does not rely on `HostConfig.NetworkMode` being present in the inspect
/// response, so it works with runtimes such as socktainer.
pub async fn container_bridge_ip(
    container_id: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let docker = Docker::connect_with_defaults()?;

    let inspect = docker
        .inspect_container(container_id, None::<InspectContainerOptions>)
        .await?;

    let settings = inspect
        .network_settings
        .ok_or("container has no NetworkSettings")?;
    let networks = settings.networks.ok_or("container has no Networks")?;
    let network = networks
        .values()
        .next()
        .ok_or("container is not attached to any network")?;

    network
        .ip_address
        .clone()
        .ok_or_else(|| "container network has no IP address".into())
}

/// Return a URL for a container that publishes its ports to the Docker host.
///
/// Use this when the container was started with `.with_mapped_port(0, ...)`. The
/// host part is whatever address the test process must use to reach the container
/// (e.g. `localhost` with Docker Desktop, a VM IP with lima, etc.).
pub async fn container_host_url(
    container: &ContainerAsync<GenericImage>,
    port: u16,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let host = container.get_host().await?.to_string();
    let host_port = container
        .get_host_port_ipv4(ContainerPort::Tcp(port))
        .await?;
    Ok(format!("http://{host}:{host_port}"))
}

/// Return a URL for a container using its bridge-network IP and original container port.
///
/// Use this when the container is reachable without published ports (the default for
/// most `sunbeam-test` builders).
#[allow(dead_code)]
pub async fn container_bridge_url(
    container: &ContainerAsync<GenericImage>,
    port: u16,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let host = container_bridge_ip(container.id()).await?;
    Ok(format!("http://{host}:{port}"))
}
