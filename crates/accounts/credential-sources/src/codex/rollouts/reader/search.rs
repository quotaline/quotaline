//! Advances a bounded reverse read across candidate rollout files.

use super::{Cache, CacheState, MAX_LINE_BYTES, Reading, ReverseRead, RolloutReader, Search, parse_line};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    time::Instant,
};

const CHUNK_BYTES: u64 = 64 * 1024;

impl RolloutReader {
    pub(super) fn advance_search(&mut self, started: Instant) {
        let Some(mut search) = self.search.take() else { return };
        let mut bytes_remaining = self.budget.bytes as u64;
        while search.file_index < search.files.len() && bytes_remaining > 0 && started.elapsed() < self.budget.time {
            let file = &search.files[search.file_index];
            let mut current = search.current.take().unwrap_or_else(|| ReverseRead {
                cursor: file.size,
                floor: file
                    .size
                    .saturating_sub(self.policy.maximum_tail_bytes.unwrap_or(u64::MAX)),
                reversed_line: Vec::new(),
                oversized: false,
            });
            let Ok(mut handle) = File::open(&file.path) else {
                search.file_index += 1;
                continue;
            };
            self.workload.files_opened += 1;
            let mut found = None;
            let mut stale = false;
            while current.cursor > current.floor && bytes_remaining > 0 && started.elapsed() < self.budget.time {
                let count = (current.cursor - current.floor).min(bytes_remaining).min(CHUNK_BYTES);
                let start = current.cursor - count;
                let mut chunk = vec![0; count as usize];
                if handle
                    .seek(SeekFrom::Start(start))
                    .and_then(|_| handle.read_exact(&mut chunk))
                    .is_err()
                {
                    stale = true;
                    break;
                }
                self.workload.bytes_read += count;
                bytes_remaining -= count;
                current.cursor = start;
                for byte in chunk.into_iter().rev() {
                    if byte == b'\n' {
                        if !current.oversized {
                            found = parse_line(&current.reversed_line, search.reference_now, &file.path, self.policy);
                            if found.is_some() {
                                break;
                            }
                        }
                        current.reversed_line.clear();
                        current.oversized = false;
                    } else if current.reversed_line.len() < MAX_LINE_BYTES {
                        current.reversed_line.push(byte);
                    } else {
                        current.oversized = true;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            if let Some(reading) = found {
                self.complete_search(search, Some(reading));
                return;
            }
            if stale {
                search.file_index += 1;
            } else if current.cursor == current.floor {
                if !current.oversized
                    && current.floor == 0
                    && let Some(reading) =
                        parse_line(&current.reversed_line, search.reference_now, &file.path, self.policy)
                {
                    self.complete_search(search, Some(reading));
                    return;
                }
                search.file_index += 1;
            } else {
                search.current = Some(current);
            }
        }
        if search.file_index == search.files.len() {
            self.complete_search(search, None);
        } else {
            self.search = Some(search);
        }
    }

    fn complete_search(&mut self, search: Search, reading: Option<Reading>) {
        self.cache = Some(Cache {
            files: search.files,
            expires_at: search.expires_at,
            state: CacheState::Searched(reading),
        });
        self.workload.searches_completed += 1;
    }
}
