//! # oxo-local
//!
//! Local backends that read logs without a remote server:
//!
//! - **File** — tail local log files (or globs like `/var/log/*.log`)
//! - **Command** — run any command and capture stdout/stderr as log lines
//! - **Docker** — stream logs from a Docker container (`docker logs -f`)
//! - **Kubernetes** — stream logs from Kubernetes pods (`kubectl logs -f`)
//!
//! ## Config examples
//!
//! ```toml
//! [[sources]]
//! name = "App Logs"
//! type = "file"
//! path = "/var/log/myapp.log"
//!
//! [[sources]]
//! name = "Node Dev"
//! type = "command"
//! command = "npm run dev"
//!
//! [[sources]]
//! name = "API Container"
//! type = "docker"
//! container = "my-api"
//!
//! [[sources]]
//! name = "K8s API Pods"
//! type = "kubernetes"
//! selector = "app=api"
//! namespace = "default"
//! ```

mod command;
mod docker;
mod file;
mod kubernetes;
mod stdin;

pub use command::CommandBackend;
pub use docker::DockerBackend;
pub use file::FileBackend;
pub use kubernetes::KubernetesBackend;
pub use stdin::StdinBackend;
