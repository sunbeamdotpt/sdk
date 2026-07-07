//! VPN integration — daemon control and environment detection.

pub mod cmds;
pub mod env;

pub use env::VPN_K8S_PROXY;
pub use env::VPN_SOCKS_PROXY;
pub use env::vpn_daemon_socket_exists;
pub use env::vpn_state_dir;
