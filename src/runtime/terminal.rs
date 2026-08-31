use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

type TuiTerminal = Terminal<CrosstermBackend<io::Stdout>>;

pub(super) struct TuiSession {
    terminal: TuiTerminal,
    active: bool,
}

impl TuiSession {
    pub(super) fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        ) {
            let mut cleanup = io::stdout();
            let _ = execute!(
                cleanup,
                DisableBracketedPaste,
                DisableMouseCapture,
                LeaveAlternateScreen
            );
            let _ = disable_raw_mode();
            return Err(error);
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut cleanup = io::stdout();
                let _ = execute!(
                    cleanup,
                    DisableBracketedPaste,
                    DisableMouseCapture,
                    LeaveAlternateScreen
                );
                let _ = disable_raw_mode();
                return Err(error);
            }
        };
        Ok(Self {
            terminal,
            active: true,
        })
    }

    pub(super) fn draw<F>(&mut self, draw: F) -> io::Result<()>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.terminal.draw(draw).map(|_| ())
    }

    pub(super) fn suspend(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        )?;
        self.terminal.show_cursor()?;
        self.active = false;
        Ok(())
    }

    pub(super) fn resume(&mut self) -> io::Result<()> {
        if self.active {
            return Ok(());
        }
        enable_raw_mode()?;
        if let Err(error) = execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        ) {
            let _ = execute!(
                self.terminal.backend_mut(),
                DisableBracketedPaste,
                DisableMouseCapture,
                LeaveAlternateScreen
            );
            let _ = disable_raw_mode();
            return Err(error);
        }
        if let Err(error) = self.terminal.clear() {
            let _ = execute!(
                self.terminal.backend_mut(),
                DisableBracketedPaste,
                DisableMouseCapture,
                LeaveAlternateScreen
            );
            let _ = disable_raw_mode();
            return Err(error);
        }
        self.active = true;
        Ok(())
    }
}

impl Drop for TuiSession {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
            let _ = execute!(
                self.terminal.backend_mut(),
                DisableBracketedPaste,
                DisableMouseCapture,
                LeaveAlternateScreen
            );
            let _ = self.terminal.show_cursor();
            self.active = false;
        }
    }
}

pub(super) fn wait_for_terminal_line(shutdown: &AtomicBool) -> io::Result<()> {
    loop {
        if shutdown.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Line was asked to shut down",
            ));
        }
        let mut descriptor = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, 250) };
        if result > 0 && descriptor.revents & libc::POLLIN != 0 {
            return Ok(());
        }
        if result > 0 && descriptor.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "terminal input closed while waiting for confirmation",
            ));
        }
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}
