//! Testcontainers-backed helpers for integration tests.
//!
//! Postgres and OpenBao come from the sdk's testcontainer builders
//! (`sdk::testing`); NATS stays local because the sdk has no NATS builder.
//! Each helper lazily starts a container once per test binary and returns a
//! connection URL. If the corresponding environment variable is set
//! (`DATABASE_URL`, `NATS_URL`, `VAULT_ADDR`), the container is skipped and
//! the external URL is used instead.

#![cfg(all(feature = "testing", feature = "g2v-sqlx"))]
#![allow(dead_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test support may unwrap freely (SSO-027/G2V-003)

use std::sync::Once;
use std::time::Duration;

use sqlx::Connection as _;
use testcontainers::core::{ContainerPort, IntoContainerPort};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::Mutex;

/// Pin `aws-lc-rs` as the process-wide rustls crypto provider.
///
/// Dev builds can unify *both* rustls provider features — the sdk's
/// testcontainers dependency still enables the `ring` flavor while every
/// other edge selects `aws-lc-rs` — and rustls refuses to auto-select a
/// provider when both are present. Installing the default explicitly keeps
/// TLS connections (Docker daemon, Postgres, NATS, ...) deterministic.
/// Best-effort: if a provider is already installed, that one stays.
pub fn install_default_crypto_provider() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .is_err()
        {
            eprintln!("rustls crypto provider already installed; keeping existing default");
        }
    });
}

/// Prepare the container-test environment before any container starts.
///
/// Ensures `DOCKER_HOST` points at the active Docker context if it is not
/// already set. Testcontainers reads `DOCKER_HOST` directly; on macOS the
/// Docker socket is often not the default `/var/run/docker.sock` (lima,
/// colima, etc.).
///
/// For TLS-secured remote daemons (Docker contexts carrying `ca.pem` /
/// `cert.pem` / `key.pem`), the material is mirrored into the classic
/// `DOCKER_CERT_PATH` / `DOCKER_TLS_VERIFY` variables and the endpoint is
/// rewritten to the `https://` scheme — testcontainers and bollard do not
/// read Docker context TLS material on their own. Also pins the rustls
/// crypto provider (see [`install_default_crypto_provider`]).
pub fn init_docker_host() {
    install_default_crypto_provider();

    if std::env::var("DOCKER_HOST").is_ok() {
        return;
    }

    let output = std::process::Command::new("docker")
        .args([
            "context",
            "inspect",
            "-f",
            "{{.Endpoints.docker.Host}}\n{{.Storage.TLSPath}}",
        ])
        .output();

    let (mut host, tls_dir) = match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
            let host = lines.next().unwrap_or_default().to_string();
            let tls = lines.next().unwrap_or_default().to_string();
            (host, tls)
        }
        _ => (String::new(), String::new()),
    };

    if host.is_empty() {
        // Fall back to the default unix socket. If Docker isn't there, testcontainers
        // will fail with a clear connection error.
        unsafe { std::env::set_var("DOCKER_HOST", "unix:///var/run/docker.sock") };
        return;
    }

    let ca = std::path::Path::new(&tls_dir).join("docker").join("ca.pem");
    if host.starts_with("tcp://") && ca.exists() {
        if std::env::var_os("DOCKER_CERT_PATH").is_none() {
            unsafe {
                std::env::set_var("DOCKER_CERT_PATH", ca.parent().unwrap());
                std::env::set_var("DOCKER_TLS_VERIFY", "1");
            }
        }
        host = host.replacen("tcp://", "https://", 1);
    }

    unsafe { std::env::set_var("DOCKER_HOST", host) };
}

/// A started container plus the URL clients should connect to.
struct RunningContainer {
    /// Kept alive for the lifetime of the test process.
    #[allow(dead_code)]
    container: ContainerAsync<GenericImage>,
    url: String,
}

static POSTGRES: Mutex<Option<RunningContainer>> = Mutex::const_new(None);
static NATS: Mutex<Option<RunningContainer>> = Mutex::const_new(None);
static OPENBAO: Mutex<Option<RunningContainer>> = Mutex::const_new(None);

/// Resolve the host and mapped port for an exposed container port.
async fn host_port(container: &ContainerAsync<GenericImage>, port: u16) -> (String, u16) {
    let host = container
        .get_host()
        .await
        .expect("failed to resolve container host")
        .to_string();
    let mapped = container
        .get_host_port_ipv4(ContainerPort::Tcp(port))
        .await
        .expect("failed to resolve container host port");
    (host, mapped)
}

/// Return a Postgres URL, starting a container via the sdk builder if needed.
pub async fn postgres_url() -> String {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return url;
    }

    init_docker_host();

    let mut guard = POSTGRES.lock().await;
    if let Some(running) = guard.as_ref() {
        return running.url.clone();
    }

    let container = sdk::testing::Postgres::new()
        .publish_port()
        .start()
        .await
        .expect("failed to start Postgres container");
    let url = sdk::testing::Postgres::url(&container)
        .await
        .expect("failed to resolve Postgres URL");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if sqlx::PgPool::connect(&url).await.is_ok() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("Postgres container did not become ready in time: {url}");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let url_clone = url.clone();
    *guard = Some(RunningContainer {
        container,
        url: url_clone,
    });
    url
}

/// Return a Postgres pool connected to the test container.
pub async fn postgres_pool() -> Result<sqlx::PgPool, sqlx::Error> {
    let url = postgres_url().await;
    sqlx::PgPool::connect(&url).await
}

/// Poll a Postgres URL until it accepts connections or the timeout elapses.
pub async fn wait_for_postgres(
    url: &str,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match sqlx::postgres::PgConnection::connect(url).await {
            Ok(mut conn) => {
                if sqlx::query("SELECT 1").execute(&mut conn).await.is_ok() {
                    return Ok(());
                }
            }
            Err(err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(err.into());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Return a NATS URL, starting a container if needed.
///
/// The sdk has no NATS builder, so this one stays a local `GenericImage`.
pub async fn nats_url() -> String {
    if let Ok(url) = std::env::var("NATS_URL") {
        return url;
    }

    init_docker_host();

    let mut guard = NATS.lock().await;
    if let Some(running) = guard.as_ref() {
        return running.url.clone();
    }

    let container = GenericImage::new("nats", "2.10-alpine")
        .with_exposed_port(4222.tcp())
        .with_env_var("NATS_JETSTREAM", "true")
        .with_cmd(vec!["--js".to_string()])
        .with_startup_timeout(Duration::from_secs(60))
        .start()
        .await
        .expect("failed to start NATS container");

    let (host, port) = host_port(&container, 4222).await;
    let url = format!("nats://{host}:{port}");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if async_nats::connect(&url).await.is_ok() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("NATS container did not become ready in time: {url}");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let url_clone = url.clone();
    *guard = Some(RunningContainer {
        container,
        url: url_clone,
    });
    url
}

/// Return an OpenBao URL, starting a container via the sdk builder if needed.
pub async fn vault_url() -> String {
    if let Ok(url) = std::env::var("VAULT_ADDR") {
        return url;
    }

    init_docker_host();

    let mut guard = OPENBAO.lock().await;
    if let Some(running) = guard.as_ref() {
        return running.url.clone();
    }

    let container = sdk::testing::OpenBao::new()
        .publish_ports()
        .start()
        .await
        .expect("failed to start OpenBao container");
    let url = sdk::testing::OpenBao::url(&container)
        .await
        .expect("failed to resolve OpenBao URL");

    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(resp) = client.get(format!("{url}/v1/sys/health")).send().await
            && resp.status().is_success()
        {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("OpenBao container did not become ready in time: {url}");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let url_clone = url.clone();
    *guard = Some(RunningContainer {
        container,
        url: url_clone,
    });
    url
}

/// Return a `VaultConfig` pointing at the test OpenBao container.
pub async fn vault_test_config() -> sdk::g2v::config::VaultConfig {
    let url = vault_url().await;
    sdk::g2v::config::VaultConfig {
        url,
        token: sdk::testing::OpenBao::DEFAULT_ROOT_TOKEN.to_string(),
        kv_path: "secret/g2v-elections".to_string(),
        lease_duration: 30,
    }
}
