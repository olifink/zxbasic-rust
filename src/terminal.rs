//! Keyboard access while a program runs: BREAK detection and `INKEY$`.
//!
//! When stdin is an interactive terminal, the console switches it to a
//! non-canonical, no-echo, no-signal mode for the duration of a run, so that
//! Ctrl+C arrives as a byte (BREAK) instead of killing the process, and
//! single key presses can be read without waiting for Enter. The original
//! settings are restored when the run ends, before `INPUT`, and on drop.

use std::collections::VecDeque;
use std::io;
use std::os::fd::AsFd;

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::termios::{self, LocalFlags, SetArg, SpecialCharacterIndices, Termios};

const CTRL_C: u8 = 0x03;

/// The user pressed Ctrl+C while a program was running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Break;

pub struct Console {
    /// Saved terminal settings; `None` when stdin is not a terminal.
    original: Option<Termios>,
    raw: bool,
    keys: VecDeque<u8>,
}

impl Console {
    /// A console that never touches the terminal (pipes, files, tests).
    pub fn disabled() -> Self {
        Console {
            original: None,
            raw: false,
            keys: VecDeque::new(),
        }
    }

    /// A console attached to stdin, which must be a terminal.
    pub fn for_stdin() -> Self {
        Console {
            original: termios::tcgetattr(io::stdin().as_fd()).ok(),
            raw: false,
            keys: VecDeque::new(),
        }
    }

    /// Switches to run mode: keys are read one at a time, unechoed, and
    /// Ctrl+C no longer raises SIGINT.
    pub fn enter_run_mode(&mut self) {
        let Some(original) = &self.original else {
            return;
        };
        let mut raw = original.clone();
        raw.local_flags
            .remove(LocalFlags::ICANON | LocalFlags::ECHO | LocalFlags::ISIG);
        raw.control_chars[SpecialCharacterIndices::VMIN as usize] = 1;
        raw.control_chars[SpecialCharacterIndices::VTIME as usize] = 0;
        if termios::tcsetattr(io::stdin().as_fd(), SetArg::TCSANOW, &raw).is_ok() {
            self.raw = true;
        }
        self.keys.clear();
    }

    /// Restores the original terminal settings.
    pub fn leave_run_mode(&mut self) {
        if let (true, Some(original)) = (self.raw, &self.original) {
            let _ = termios::tcsetattr(io::stdin().as_fd(), SetArg::TCSANOW, original);
            self.raw = false;
        }
        self.keys.clear();
    }

    /// Collects pending key presses; returns true if Ctrl+C was pressed.
    pub fn poll_break(&mut self) -> bool {
        if !self.raw {
            return false;
        }
        let stdin = io::stdin();
        loop {
            let mut fds = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
            match poll(&mut fds, PollTimeout::ZERO) {
                Ok(n) if n > 0 => {}
                _ => break,
            }
            let mut buf = [0u8; 64];
            match nix::unistd::read(stdin.as_fd(), &mut buf) {
                Ok(n) if n > 0 => {
                    for &b in &buf[..n] {
                        if b == CTRL_C {
                            self.keys.clear();
                            return true;
                        }
                        self.keys.push_back(b);
                    }
                }
                _ => break,
            }
        }
        false
    }

    /// Next pending key for `INKEY$`, if any.
    pub fn inkey(&mut self) -> Result<Option<char>, Break> {
        if self.poll_break() {
            return Err(Break);
        }
        Ok(self.keys.pop_front().map(char::from))
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        self.leave_run_mode();
    }
}
