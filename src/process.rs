#[cfg(all(feature = "smol", not(feature = "tokio")))]
use smol::io::AsyncReadExt;
use std::fs::File;
use std::io;
use std::os::fd::OwnedFd;
use std::process::{exit, Command, Stdio};
#[cfg(feature = "tokio")]
use tokio::io::AsyncReadExt;

#[cfg(feature = "tokio")]
async fn read_from_pipe(read: OwnedFd) -> Option<u32> {
    let mut read = tokio::net::unix::pipe::Receiver::from_owned_fd(read).unwrap();
    read.read_u32().await.ok()
}

#[cfg(all(feature = "smol", not(feature = "tokio")))]
async fn read_from_pipe(read: OwnedFd) -> Option<u32> {
    let mut read = smol::Async::new(std::fs::File::from(read)).unwrap();
    let mut bytes = [0; 4];
    read.read_exact(&mut bytes).await.ok()?;
    Some(u32::from_be_bytes(bytes))
}

/// Performs a double fork with setsid to spawn and detach a command.
pub async fn spawn(mut command: Command) -> Option<u32> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let Ok((read, write)) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC) else {
        return None;
    };

    match unsafe { libc::fork() } {
        // Parent process
        1.. => {
            // Drop copy of write end, then read PID from pipe
            drop(write);
            read_from_pipe(read).await
        }

        // Child process
        0 => {
            let _res = rustix::process::setsid();
            if let Ok(child) = command.spawn() {
                // Write PID to pipe
                let _ = rustix::io::write(write, &child.id().to_be_bytes());
            }

            exit(0)
        }

        ..=-1 => {
            println!(
                "failed to fork and spawn command: {}",
                io::Error::last_os_error()
            );

            None
        }
    }
}
