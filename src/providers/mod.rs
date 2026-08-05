pub mod platform;
pub mod system;

#[cfg(feature = "containers")]
pub mod containers;

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    time::timeout,
};

use crate::domain::ProviderError;

const MAX_COMMAND_OUTPUT: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub async fn run_command<I, S>(
    program: &Path,
    args: I,
    deadline: Duration,
) -> Result<CommandOutput, ProviderError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| ProviderError::Io(error.to_string()))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProviderError::Io("missing stdout pipe".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| ProviderError::Io("missing stderr pipe".into()))?;
    let read = async {
        let out = drain_bounded(&mut stdout, MAX_COMMAND_OUTPUT);
        let err = drain_bounded(&mut stderr, MAX_COMMAND_OUTPUT);
        let (out_result, err_result, status_result) = tokio::join!(out, err, child.wait(),);
        let out = out_result.map_err(|error| ProviderError::Io(error.to_string()))?;
        let err = err_result.map_err(|error| ProviderError::Io(error.to_string()))?;
        let status = status_result.map_err(|error| ProviderError::Io(error.to_string()))?;
        Ok::<_, ProviderError>(CommandOutput {
            code: status.code(),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    };
    timeout(deadline, read)
        .await
        .map_err(|_| ProviderError::Timeout(program.display().to_string()))?
}

async fn drain_bounded<R>(reader: &mut R, limit: usize) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut kept = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(kept)
}

pub fn find_trusted_command(name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    const ROOTS: &[&str] = &[r"C:\Windows\System32", r"C:\Windows"];
    #[cfg(not(target_os = "windows"))]
    const ROOTS: &[&str] = &["/usr/bin", "/usr/sbin", "/bin", "/sbin"];

    ROOTS
        .iter()
        .map(|root| Path::new(root).join(name))
        .find(|path| path.is_file())
}
