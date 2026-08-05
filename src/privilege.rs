//! Safe privilege-broker invocation.
//!
//! The caller must leave raw mode before invoking this module. Password input
//! belongs to the inherited terminal owned by `sudo` or `doas`; the monitor
//! never receives or stores it.

use std::{ffi::OsStr, io, path::Path, process::ExitStatus};

#[cfg(target_os = "linux")]
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

#[cfg(target_os = "linux")]
const BROKERS: &[&str] = &["/usr/bin/doas", "/usr/bin/sudo", "/bin/doas", "/bin/sudo"];
#[cfg(any(target_os = "linux", test))]
const ALLOWED_TARGET_ROOTS: &[&str] = &["/usr/bin", "/usr/sbin", "/bin", "/sbin"];

#[derive(Debug, thiserror::Error)]
pub enum PrivilegeError {
    #[error("no trusted sudo/doas executable was found")]
    BrokerUnavailable,
    #[error("privileged target is not a trusted absolute executable: {0}")]
    UntrustedTarget(String),
    #[error("failed to invoke privilege broker: {0}")]
    Io(#[from] io::Error),
}

#[cfg(target_os = "linux")]
pub fn execute<I, S>(target: &Path, args: I) -> Result<ExitStatus, PrivilegeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    validate_target(target)?;
    let broker = BROKERS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or(PrivilegeError::BrokerUnavailable)?;

    Command::new(broker)
        .arg("--")
        .arg(target)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(PrivilegeError::from)
}

#[cfg(not(target_os = "linux"))]
pub fn execute<I, S>(_target: &Path, _args: I) -> Result<ExitStatus, PrivilegeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Err(PrivilegeError::BrokerUnavailable)
}

#[cfg(any(target_os = "linux", test))]
fn validate_target(target: &Path) -> Result<(), PrivilegeError> {
    if !target.is_absolute() || !target.is_file() {
        return Err(PrivilegeError::UntrustedTarget(
            target.display().to_string(),
        ));
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("/"));
    if !ALLOWED_TARGET_ROOTS
        .iter()
        .any(|root| parent == Path::new(root))
    {
        return Err(PrivilegeError::UntrustedTarget(
            target.display().to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_targets_are_rejected() {
        let result = validate_target(Path::new("sudo"));
        assert!(matches!(result, Err(PrivilegeError::UntrustedTarget(_))));
    }
}
