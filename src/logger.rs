//! Tiny rotating file logger (PRD §11). std-only, no unsafe.
//!
//! `log` facade + one file sink: `litewall.log`, rotated to `.1`..`.N` when it
//! would exceed `MAX_BYTES`. Errors while logging are swallowed on purpose.
use log::{LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const FILE_NAME: &str = "litewall.log";
const MAX_BYTES: u64 = 512 * 1024;
const KEEP: usize = 3;

fn numbered(path: &Path, n: usize) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

fn open_append(path: &Path) -> io::Result<(File, u64)> {
    let f = OpenOptions::new().create(true).append(true).open(path)?;
    let size = f.metadata()?.len();
    Ok((f, size))
}

struct FileSink {
    path: PathBuf,
    file: Option<File>,
    size: u64,
    max_bytes: u64,
    keep: usize,
}

impl FileSink {
    fn new(path: PathBuf, max_bytes: u64, keep: usize) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let (file, size) = open_append(&path)?;
        Ok(Self {
            path,
            file: Some(file),
            size,
            max_bytes,
            keep,
        })
    }

    /// Close, shift `.i` -> `.i+1`, move the live file to `.1`, reopen empty.
    fn rotate(&mut self) -> io::Result<()> {
        self.file = None; // Windows cannot rename an open file
        if self.keep > 0 {
            let _ = fs::remove_file(numbered(&self.path, self.keep));
            for i in (1..self.keep).rev() {
                let from = numbered(&self.path, i);
                if from.exists() {
                    let _ = fs::rename(&from, numbered(&self.path, i + 1));
                }
            }
            let _ = fs::rename(&self.path, numbered(&self.path, 1));
        } else {
            let _ = fs::remove_file(&self.path);
        }
        let (f, s) = open_append(&self.path)?;
        self.file = Some(f);
        self.size = s;
        Ok(())
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        let len = line.len() as u64 + 1;
        if self.size > 0 && self.size + len > self.max_bytes {
            self.rotate()?;
        }
        if self.file.is_none() {
            let (f, s) = open_append(&self.path)?;
            self.file = Some(f);
            self.size = s;
        }
        let f = self.file.as_mut().expect("opened above");
        // writeln!(f, "{line}")?;
        // One write call per record: first and second launch append to the same file.
        f.write_all(format!("{line}\n").as_bytes())?;
        self.size += len;
        Ok(())
    }
}

/// `YYYY-MM-DD HH:MM:SS` (UTC) from Unix seconds. Civil-from-days algorithm.
fn utc_string(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn now_string() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", utc_string(d.as_secs()), d.subsec_millis())
}

struct Logger {
    sink: Mutex<FileSink>,
    level: LevelFilter,
}

impl Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        m.level() <= self.level
    }
    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!("{} {:<5} {}", now_string(), r.level(), r.args());
        let mut sink = self.sink.lock().unwrap_or_else(|p| p.into_inner());
        let _ = sink.write_line(&line);
    }
    fn flush(&self) {
        let mut sink = self.sink.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(f) = sink.file.as_mut() {
            let _ = f.flush();
        }
    }
}

/// Install the global logger writing to `<dir>/litewall.log`.
/// Fails if the file cannot be opened or a logger is already installed.
pub fn init(dir: &Path, level: LevelFilter) -> io::Result<()> {
    let sink = FileSink::new(dir.join(FILE_NAME), MAX_BYTES, KEEP)?;
    log::set_boxed_logger(Box::new(Logger {
        sink: Mutex::new(sink),
        level,
    }))
    .map_err(|e| io::Error::new(io::ErrorKind::AlreadyExists, e.to_string()))?;
    log::set_max_level(level);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("litewall-log-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn utc_formatting() {
        assert_eq!(utc_string(0), "1970-01-01 00:00:00");
        assert_eq!(utc_string(1_700_000_000), "2023-11-14 22:13:20");
        assert_eq!(utc_string(951_782_400), "2000-02-29 00:00:00"); // leap day
    }

    #[test]
    fn rotation_keeps_n_files() {
        let d = tmp_dir("rot");
        let p = d.join("x.log");
        let mut s = FileSink::new(p.clone(), 50, 2).unwrap();
        for _ in 0..10 {
            s.write_line("0123456789012345678").unwrap(); // 20 bytes with newline
        }
        assert!(p.exists());
        assert!(numbered(&p, 1).exists());
        assert!(numbered(&p, 2).exists());
        assert!(!numbered(&p, 3).exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn reopen_appends_and_counts_size() {
        let d = tmp_dir("reopen");
        let p = d.join("x.log");
        {
            let mut s = FileSink::new(p.clone(), 1000, 2).unwrap();
            s.write_line("hello").unwrap();
        }
        let s = FileSink::new(p.clone(), 1000, 2).unwrap();
        assert_eq!(s.size, 6);
        assert_eq!(fs::read_to_string(&p).unwrap(), "hello\n");
        let _ = fs::remove_dir_all(d);
    }
}
