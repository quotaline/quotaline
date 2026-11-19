//! Loads and rotates diagnostic text files.

use quotaline_logging::parse::ParsedLine;
use quotaline_logging::parse::parse_lines;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub struct LoadedLogs {
    pub entries: Vec<ParsedLine>,
    pub dropped_entries: bool,
}

pub fn load(
    path: &Path,
    count: usize,
    maximum_file_bytes: usize,
    maximum_entries: usize,
    cutoff: f64,
    home: Option<&str>,
) -> LoadedLogs {
    let mut text = String::new();
    let mut dropped = false;
    for index in (0..count).rev() {
        let file = archive(path, index);
        if let Ok((part, truncated)) = bounded_contents(&file, maximum_file_bytes) {
            text.push_str(&part);
            text.push('\n');
            dropped |= truncated;
        }
    }
    let lines = text.split('\n').filter(|line| !line.is_empty()).collect::<Vec<_>>();
    dropped |= lines.len() > maximum_entries;
    let selected = lines
        .iter()
        .rev()
        .take(maximum_entries)
        .rev()
        .map(|line| (*line).to_owned())
        .collect::<Vec<_>>();
    let entries = parse_lines(&selected, cutoff, cutoff, home);
    dropped |= entries.len() < selected.len();
    LoadedLogs {
        entries,
        dropped_entries: dropped,
    }
}

pub fn append(path: &Path, lines: &[String], maximum_file_bytes: usize, count: usize) -> io::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let mut size = fs::metadata(path).map(|metadata| metadata.len() as usize).unwrap_or(0);
    let mut file = open(path)?;
    for line in lines {
        let data = file_line(line, maximum_file_bytes);
        if size > 0 && size + data.len() > maximum_file_bytes {
            drop(file);
            rotate(path, count)?;
            file = open(path)?;
            size = 0;
        }
        file.write_all(&data)?;
        size += data.len();
    }
    Ok(())
}

pub fn rewrite(path: &Path, lines: &[String], maximum_file_bytes: usize, count: usize) -> io::Result<()> {
    clear(path, count)?;
    append(path, lines, maximum_file_bytes, count)
}

pub fn clear(path: &Path, count: usize) -> io::Result<()> {
    for index in 0..count {
        ignore_missing(fs::remove_file(archive(path, index)))?;
    }
    Ok(())
}

fn ignore_missing(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn bounded_contents(path: &Path, maximum_bytes: usize) -> io::Result<(String, bool)> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let offset = size.saturating_sub(maximum_bytes.max(1) as u64);
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(size - offset).read_to_end(&mut bytes)?;
    let mut text = String::from_utf8_lossy_owned(bytes);
    if offset > 0 {
        if let Some(newline) = text.find('\n') {
            text.drain(..=newline);
        } else {
            text.clear();
        }
    }
    Ok((text, offset > 0))
}

fn file_line(line: &str, maximum_bytes: usize) -> Vec<u8> {
    let mut data = format!("{line}\n").into_bytes();
    if data.len() <= maximum_bytes {
        return data;
    }
    if maximum_bytes <= 1 {
        return vec![b'\n'; maximum_bytes];
    }
    data = line.as_bytes()[..line.floor_char_boundary(maximum_bytes - 1)].to_vec();
    data.push(b'\n');
    data
}

fn open(path: &Path) -> io::Result<File> {
    crate::open_private(OpenOptions::new().create(true).append(true), path)
}

fn rotate(path: &Path, count: usize) -> io::Result<()> {
    if count == 1 {
        return clear(path, 1);
    }
    for index in (1..count).rev() {
        let source = archive(path, index - 1);
        let destination = archive(path, index);
        ignore_missing(fs::remove_file(&destination))?;
        ignore_missing(fs::rename(source, destination))?;
    }
    Ok(())
}

fn archive(path: &Path, index: usize) -> PathBuf {
    if index == 0 {
        path.to_path_buf()
    } else {
        PathBuf::from(format!("{}.{index}", path.display()))
    }
}

#[cfg(test)]
#[path = "../tests/unit/logs.rs"]
mod tests;
