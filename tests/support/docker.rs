//! Docker environment detection for integration tests.
//!
//! Testcontainers reads `DOCKER_HOST` directly; on macOS the Docker socket is
//! often not the default `/var/run/docker.sock` (lima, colima, etc.). This
//! helper ensures `DOCKER_HOST` points at the active Docker context before the
//! first container is started.

/// Ensure `DOCKER_HOST` points at the active Docker context if it is not
/// already set.
pub fn init_docker_host() {
    if std::env::var("DOCKER_HOST").is_ok() {
        return;
    }

    let output = std::process::Command::new("docker")
        .args(["context", "inspect", "-f", "{{.Endpoints.docker.Host}}"])
        .output();

    if let Ok(output) = output
        && output.status.success()
    {
        let host = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !host.is_empty() {
            unsafe { std::env::set_var("DOCKER_HOST", host) };
            return;
        }
    }

    // Fall back to the default unix socket. If Docker isn't there,
    // testcontainers will fail with a clear connection error.
    unsafe { std::env::set_var("DOCKER_HOST", "unix:///var/run/docker.sock") };
}
