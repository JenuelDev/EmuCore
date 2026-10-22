//! Windows process-tree ownership for external-engine lifecycle work.
#![forbid(unsafe_code)]

use crate::{Result, native::process::JobProcess};
use std::{
    process::{ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

/// Owns a hidden child and its ordinary CreateProcess descendants until dropped.
/// This is lifecycle management, not an isolation or security boundary.
pub struct ManagedProcess {
    inner: JobProcess,
}

/// Windows job commit-accounting peaks, not resident memory or GPU memory.
#[derive(Clone, Copy, Debug)]
pub struct JobMemory {
    pub peak_job_bytes: usize,
    pub peak_process_bytes: usize,
}

impl ManagedProcess {
    /// Creates the child suspended, assigns a private non-inheritable kill-on-close
    /// job, then resumes its primary thread. Overrides Windows creation flags.
    /// Configure stdin/stdout/stderr on the command before calling this method.
    pub fn spawn(command: &mut Command) -> Result<Self> {
        JobProcess::spawn(command).map(|inner| Self { inner })
    }

    pub fn id(&self) -> u32 {
        self.inner.child.id()
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.inner
            .child
            .try_wait()
            .map_err(|error| crate::Error::io("wait for owned process", error))
    }

    /// A timeout returns None; the caller can continue observing or terminate.
    /// Dropping this owner always releases its job and stops remaining descendants.
    pub fn wait_for(&mut self, timeout: Duration) -> Result<Option<ExitStatus>> {
        let started = Instant::now();
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(Some(status));
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Ok(None);
            }
            thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }

    /// Terminates the entire owned job, even if its original child already exited.
    pub fn terminate(&mut self) -> Result<ExitStatus> {
        self.inner.terminate()
    }

    pub fn memory_peaks(&self) -> Result<JobMemory> {
        let (peak_job_bytes, peak_process_bytes) = self.inner.memory_peaks()?;
        Ok(JobMemory {
            peak_job_bytes,
            peak_process_bytes,
        })
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.inner.child.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.inner.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.inner.child.stderr.take()
    }
}
