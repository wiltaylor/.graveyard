use std::collections::HashMap;
use std::ops::Sub;
use std::process::{Stdio};
use std::time::{Duration, SystemTime};
use tokio::process::{Command, Child};
use tokio_stream::Stream;
use tokio_util::codec::{FramedRead, LinesCodec};
use anyhow::Result;
use tokio::time::timeout;
use tokio_stream::StreamExt;

pub struct RunningCommand {
    time_left: Option<Duration>,
    _process: Child,
    stream: Box<dyn Stream<Item = String> + Unpin>,
}

impl RunningCommand {
    pub fn new(command: &String, environment: &HashMap<String, String>, timeout: Option<Duration>) -> Result<Box<RunningCommand>> {
        let mut process = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .envs(environment)
                .args(["/C", command])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .spawn()?
        } else {
            Command::new("sh")
                .envs(environment)
                .args(["-c", command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .spawn()?
        };

        let stdout = FramedRead::new(process.stdout.take().unwrap(), LinesCodec::new())
            .map(|data| data.expect("fail on out!"));

        let stderr = FramedRead::new(process.stderr.take().unwrap(), LinesCodec::new())
            .map(|data| data.expect("fail on err!"));

        let stream: Box<dyn Stream<Item = String> + Unpin> = Box::new(stdout.chain(stderr));
        
        Ok(Box::new(RunningCommand{
            time_left: timeout,
            _process: process,
            stream,
        }))
    }

    pub async fn next_line(&mut self) -> Result<Option<String>> {
        if let Some(time_left) = self.time_left {
            let start_time = SystemTime::now();
            let result = timeout(time_left, self.stream.next()).await?;
            self.time_left = Some(time_left.sub(SystemTime::now().duration_since(start_time)?));
            Ok(result)
        } else{
            Ok(self.stream.next().await)
        }
    }
}