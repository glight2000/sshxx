//! Terminal driver, which communicates with a shell subprocess through PTY.

#![allow(unsafe_code)]

cfg_if::cfg_if! {
    if #[cfg(unix)] {
        mod unix;
        pub use unix::{get_default_shell, Terminal};
    } else if #[cfg(windows)] {
        mod windows;
        pub use windows::{get_default_shell, Terminal};
    } else {
        compile_error!("unsupported platform for terminal driver");
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    #[cfg(unix)]
    use tokio::io::AsyncWriteExt;
    #[cfg(unix)]
    use tokio::time::{sleep, Duration};

    use super::Terminal;

    #[tokio::test]
    async fn winsize() -> Result<()> {
        let shell = if cfg!(unix) { "/bin/sh" } else { "cmd.exe" };
        let mut terminal = Terminal::new(shell, &[], None).await?;
        assert_eq!(terminal.get_winsize()?, (0, 0));
        terminal.set_winsize(120, 72)?;
        assert_eq!(terminal.get_winsize()?, (120, 72));
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reports_current_working_directory() -> Result<()> {
        let mut terminal = Terminal::new("/bin/sh", &[], Some(std::path::Path::new("/"))).await?;
        terminal.write_all(b"cd /tmp\n").await?;
        sleep(Duration::from_millis(100)).await;
        assert_eq!(
            terminal.working_directory().await.as_deref(),
            Some(std::path::Path::new("/tmp")),
        );
        Ok(())
    }
}
