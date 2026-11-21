use std::collections::VecDeque;

use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct RetainedBuffer {
    pub(super) head: Vec<u8>,
    pub(super) tail: VecDeque<u8>,
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
            if chunk.len() >= SESSION_TAIL_BYTES {
                self.tail.clear();
                self.tail.extend(&chunk[chunk.len() - SESSION_TAIL_BYTES..]);
            } else {
                let overflow = (self.tail.len() + chunk.len()).saturating_sub(SESSION_TAIL_BYTES);
                if overflow > 0 {
                    self.tail.drain(..overflow);
                }
                self.tail.extend(chunk);
            }
            if self.tail.capacity() > SESSION_TAIL_BYTES + 64 * 1024 {
                self.tail.shrink_to(SESSION_TAIL_BYTES);
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
            (tail_start, copy_deque_range(&self.tail, 0, take), next)
        } else {
            let start = requested_offset
                .saturating_sub(tail_start)
                .min(self.tail.len());
            let end = self.tail.len().min(start.saturating_add(limit));
            let logical_end = tail_start + end;
            let next = (logical_end < self.total_bytes).then_some(logical_end);
            (
                tail_start + start,
                copy_deque_range(&self.tail, start, end),
                next,
            )
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
        let max_bytes = max_bytes.max(1);
        if self.evicted_bytes() == 0 {
            let total = self.retained_bytes();
            if total <= max_bytes {
                let mut data = Vec::with_capacity(total);
                data.extend_from_slice(&self.head);
                let (a, b) = self.tail.as_slices();
                data.extend_from_slice(a);
                data.extend_from_slice(b);
                return Truncated {
                    content: String::from_utf8_lossy(&data).into_owned(),
                    truncated: false,
                };
            }
            let head_budget = (max_bytes / 8).max(1).min(total);
            let tail_budget = max_bytes
                .saturating_sub(head_budget)
                .min(total.saturating_sub(head_budget));
            let head_bytes = self.copy_contiguous_range(0, head_budget);
            let tail_bytes = self.copy_contiguous_range(total - tail_budget, total);
            return format_preview(&head_bytes, &tail_bytes, self.total_bytes);
        }

        let head_budget = (max_bytes / 8).max(1).min(self.head.len());
        let tail_budget = max_bytes.saturating_sub(head_budget).min(self.tail.len());
        let tail_start = self.tail.len().saturating_sub(tail_budget);
        let tail_bytes = copy_deque_range(&self.tail, tail_start, self.tail.len());
        format_preview(&self.head[..head_budget], &tail_bytes, self.total_bytes)
    }

    fn copy_contiguous_range(&self, start: usize, end: usize) -> Vec<u8> {
        if start >= end {
            return Vec::new();
        }
        let head_len = self.head.len();
        let mut out = Vec::with_capacity(end - start);
        if start < head_len {
            let head_end = end.min(head_len);
            out.extend_from_slice(&self.head[start..head_end]);
        }
        if end > head_len {
            let tail_start = start.saturating_sub(head_len);
            let tail_end = (end - head_len).min(self.tail.len());
            extend_from_deque_range(&self.tail, tail_start, tail_end, &mut out);
        }
        out
    }
}

fn copy_deque_range(deque: &VecDeque<u8>, start: usize, end: usize) -> Vec<u8> {
    if start >= end {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(end - start);
    extend_from_deque_range(deque, start, end, &mut out);
    out
}

fn extend_from_deque_range(deque: &VecDeque<u8>, start: usize, end: usize, out: &mut Vec<u8>) {
    let end = end.min(deque.len());
    if start >= end {
        return;
    }
    let (a, b) = deque.as_slices();
    let a_len = a.len();
    if start < a_len {
        let a_end = end.min(a_len);
        out.extend_from_slice(&a[start..a_end]);
    }
    if end > a_len {
        let b_start = start.saturating_sub(a_len);
        let b_end = end - a_len;
        out.extend_from_slice(&b[b_start..b_end]);
    }
}

fn format_preview(head: &[u8], tail: &[u8], total_bytes: usize) -> Truncated {
    let mut content = String::new();
    if !head.is_empty() {
        content.push_str(&String::from_utf8_lossy(head));
    }
    let omitted = total_bytes.saturating_sub(head.len() + tail.len());
    if omitted > 0 {
        content.push_str(&format!("\n...[{omitted} bytes omitted]...\n"));
    }
    if !tail.is_empty() {
        content.push_str(&String::from_utf8_lossy(tail));
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
