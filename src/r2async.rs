//! Provides functionality to connect with radare2 asynchronously.
//!
//! Please check crate level documentation for more details and example.

use crate::R2PipeSpawnOptions;
use crate::{r2pipe::process_result, Error, Result};
use serde_json::Value;
use std::{path::Path, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

/// Stores descriptors to the spawned r2 process.
pub struct R2PipeSpawnAsync {
    read: BufReader<ChildStdout>,
    write: ChildStdin,
    child: Option<Child>,
}

/// Provides abstraction between the three invocation methods.
pub enum R2PipeAsync {
    Pipe(R2PipeSpawnAsync),
}

impl R2PipeAsync {
    pub async fn cmd(&mut self, cmd: &str) -> Result<String> {
        match *self {
            R2PipeAsync::Pipe(ref mut x) => x.cmd(cmd.trim()).await,
        }
    }

    pub async fn cmdj(&mut self, cmd: &str) -> Result<Value> {
        match *self {
            R2PipeAsync::Pipe(ref mut x) => x.cmdj(cmd.trim()).await,
        }
    }

    pub async fn spawn<T: AsRef<str>>(
        name: T,
        opts: Option<R2PipeSpawnOptions>,
    ) -> Result<R2PipeAsync> {
        let exepath = match opts {
            Some(ref opt) => opt.exepath.clone(),
            _ => "r2".to_owned(),
        };
        let args = match opts {
            Some(ref opt) => opt.args.clone(),
            _ => vec![],
        };
        let path = Path::new(name.as_ref());
        let mut child = Command::new(exepath)
            .arg("-q0")
            .args(&args)
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        // If stdin/stdout is not available, hard error
        let sin = child.stdin.take().unwrap();
        let mut sout = child.stdout.take().unwrap();

        // flush out the initial null byte.
        let mut w = [0; 1];
        sout.read_exact(&mut w).await?;

        let res = R2PipeSpawnAsync {
            read: BufReader::new(sout),
            write: sin,
            child: Some(child),
        };

        Ok(R2PipeAsync::Pipe(res))
    }
}

impl R2PipeSpawnAsync {
    pub async fn cmd(&mut self, cmd: &str) -> Result<String> {
        let cmd = cmd.to_owned() + "\n";
        self.write.write_all(cmd.as_bytes()).await?;

        let mut res: Vec<u8> = Vec::new();
        self.read.read_until(0u8, &mut res).await?;
        process_result(res)
    }

    pub async fn cmdj(&mut self, cmd: &str) -> Result<Value> {
        let result = self.cmd(cmd).await?;
        if result.is_empty() {
            return Err(Error::EmptyResponse);
        }
        Ok(serde_json::from_str(&result)?)
    }

    /// Gracefully closes the spawned radare2 instance.
    pub async fn close(&mut self) {
        let _ = self.cmd("q!");
        if let Some(child) = &mut self.child {
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::R2PipeAsync;

    #[tokio::test]
    async fn spawn() {
        let mut r2 = R2PipeAsync::spawn("/bin/ls", None).await.unwrap();
        let res = r2.cmdj("ij").await;
        assert!(res.is_ok());
        let not_json = r2.cmdj("i").await;
        assert!(not_json.is_err());
    }
}
