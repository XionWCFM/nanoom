pub mod affected;
pub mod cache_key;
pub mod install;
pub mod run;
pub mod status;

use std::process::{ExitStatus, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::Command;

pub(crate) fn display_command(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .map(|part| format!("{part:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) async fn run_streamed(command: &mut Command) -> std::io::Result<ExitStatus> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (_, _, status) =
        tokio::try_join!(forward_flat(stdout), forward_flat(stderr), child.wait())?;
    Ok(status)
}

async fn forward_flat(reader: impl AsyncRead + Unpin) -> std::io::Result<()> {
    let mut lines = BufReader::new(reader).split(b'\n');
    let mut stderr = tokio::io::stderr();
    while let Some(mut line) = lines.next_segment().await? {
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let Some((prefix, content)) = flattened_line(&line) else {
            continue;
        };
        stderr.write_all(prefix).await?;
        stderr.write_all(content).await?;
        stderr.write_all(b"\n").await?;
        stderr.flush().await?;
    }
    Ok(())
}

fn flattened_line(line: &[u8]) -> Option<(&'static [u8], &[u8])> {
    if line == b"::endgroup::" {
        None
    } else if let Some(title) = line.strip_prefix(b"::group::") {
        Some(("◆ ".as_bytes(), title))
    } else {
        Some((b"", line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_groups_as_plain_headings() {
        assert_eq!(
            flattened_line(b"before"),
            Some((b"".as_slice(), b"before".as_slice()))
        );
        assert_eq!(
            flattened_line(b"::group::Resolution step"),
            Some(("◆ ".as_bytes(), b"Resolution step".as_slice()))
        );
        assert_eq!(flattened_line(b"::endgroup::"), None);
    }
}
