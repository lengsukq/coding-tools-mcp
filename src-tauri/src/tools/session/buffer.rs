use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct RetainedBuffer {
    pub(super) head: Vec<u8>,
    pub(super) tail: Vec<u8>,
    pub(super) total_bytes: usize,
}

#[derive(Debug, Clone)]
pub(super) struct RetainedPage {
    pub(super) content: Vec<u8>,
    pub(super) offset: usize,
    pub(super) next_offset: Option<usize>,
    pub(super) total_bytes: usize,
    pub(super) head_retained_bytes: usize,
    pub(super) tail_retained_bytes: usize,
    pub(super) evicted_bytes: usize,
}

impl RetainedBuffer {
    pub(super) fn append(&mut self, mut chunk: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(chunk.len());

        if self.head.len() < SESSION_HEAD_BYTES {
            let take = (SESSION_HEAD_BYTES - self.head.len()).min(chunk.len());
            self.head.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
        }

        if !chunk.is_empty() {
            self.tail.extend_from_slice(chunk);
            if self.tail.len() > SESSION_TAIL_BYTES {
                let drop = self.tail.len() - SESSION_TAIL_BYTES;
                self.tail.drain(..drop);
            }
        }
    }

    fn retained_bytes(&self) -> usize {
        self.head.len() + self.tail.len()
    }

    pub(super) fn evicted_bytes(&self) -> usize {
        self.total_bytes.saturating_sub(self.retained_bytes())
    }

    pub(super) fn tail_start(&self) -> usize {
        self.total_bytes.saturating_sub(self.tail.len())
    }

    pub(super) fn read_page(&self, requested_offset: usize, limit: usize) -> RetainedPage {
        let head_len = self.head.len();
        let tail_start = self.tail_start();

        let (offset, content, next_offset) = if requested_offset < head_len {
            let end = head_len.min(requested_offset.saturating_add(limit));
            let next = if end < head_len {
                Some(end)
            } else if tail_start < self.total_bytes {
                Some(tail_start.max(head_len))
            } else {
                None
            };
            (
                requested_offset,
                self.head[requested_offset..end].to_vec(),
                next,
            )
        } else if requested_offset < tail_start {
            let end = self.total_bytes.min(tail_start.saturating_add(limit));
            let take = end.saturating_sub(tail_start);
            let next = (end < self.total_bytes).then_some(end);
            (tail_start, self.tail[..take].to_vec(), next)
        } else {
            let start = requested_offset
                .saturating_sub(tail_start)
                .min(self.tail.len());
            let end = self.tail.len().min(start.saturating_add(limit));
            let logical_end = tail_start + end;
            let next = (logical_end < self.total_bytes).then_some(logical_end);
            (tail_start + start, self.tail[start..end].to_vec(), next)
        };

        RetainedPage {
            content,
            offset,
            next_offset,
            total_bytes: self.total_bytes,
            head_retained_bytes: self.head.len(),
            tail_retained_bytes: self.tail.len(),
            evicted_bytes: self.evicted_bytes(),
        }
    }

    pub(super) fn preview(&self, max_bytes: usize) -> Truncated {
        if self.evicted_bytes() == 0 {
            let mut data = self.head.clone();
            data.extend_from_slice(&self.tail);
            if data.len() <= max_bytes {
                return Truncated {
                    content: String::from_utf8_lossy(&data).into_owned(),
                    truncated: false,
                };
            }
            return preview_head_tail(&data, &data, self.total_bytes, max_bytes);
        }

        preview_head_tail(&self.head, &self.tail, self.total_bytes, max_bytes)
    }
}

fn preview_head_tail(head: &[u8], tail: &[u8], total_bytes: usize, max_bytes: usize) -> Truncated {
    let max_bytes = max_bytes.max(1);
    let head_budget = (max_bytes / 8).max(1).min(head.len());
    let tail_budget = max_bytes.saturating_sub(head_budget).min(tail.len());
    let mut content = String::new();
    if head_budget > 0 {
        content.push_str(&String::from_utf8_lossy(&head[..head_budget]));
    }
    let omitted = total_bytes.saturating_sub(head_budget + tail_budget);
    if omitted > 0 {
        content.push_str(&format!("\n...[{omitted} bytes omitted]...\n"));
    }
    if tail_budget > 0 {
        let start = tail.len() - tail_budget;
        content.push_str(&String::from_utf8_lossy(&tail[start..]));
    }
    Truncated {
        content,
        truncated: omitted > 0,
    }
}

pub(super) struct Truncated {
    pub(super) content: String,
    pub(super) truncated: bool,
}
