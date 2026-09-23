//! Server-sent events, as the API brains stream them: `event:` and `data:` lines, a blank line
//! ends an event. Bytes arrive in arbitrary pieces; the parser keeps partial lines.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` name, if the server sent one.
    pub event: Option<String>,
    /// The `data:` lines, joined with newlines.
    pub data: String,
}

#[derive(Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    /// Feeds bytes; returns the events they completed.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=end).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line).into_owned();
            if line.is_empty() {
                if !self.data.is_empty() || self.event.is_some() {
                    out.push(SseEvent {
                        event: self.event.take(),
                        data: std::mem::take(&mut self.data).join("\n"),
                    });
                }
                continue;
            }
            if line.starts_with(':') {
                continue; // a comment (keep-alive)
            }
            let (field, value) = line.split_once(':').unwrap_or((line.as_str(), ""));
            let value = value.strip_prefix(' ').unwrap_or(value);
            match field {
                "event" => self.event = Some(value.to_owned()),
                "data" => self.data.push(value.to_owned()),
                _ => {}
            }
        }
        out
    }

    /// What is left when the stream ends without a final blank line.
    pub fn finish(&mut self) -> Option<SseEvent> {
        let rest = std::mem::take(&mut self.buffer);
        if !rest.is_empty() {
            let mut events = self.feed(&[rest.as_slice(), b"\n\n"].concat());
            return events.pop();
        }
        if self.data.is_empty() {
            return None;
        }
        Some(SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data).join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_split_across_reads_come_out_whole() {
        let mut p = SseParser::default();
        assert!(p.feed(b"event: message_start\r\nda").is_empty());
        let events = p.feed(b"ta: {\"a\":1}\r\n\r\n: keep-alive\n\ndata: [DONE]\n\n");
        assert_eq!(
            events,
            [
                SseEvent {
                    event: Some("message_start".into()),
                    data: "{\"a\":1}".into()
                },
                SseEvent {
                    event: None,
                    data: "[DONE]".into()
                }
            ]
        );
    }

    #[test]
    fn multi_line_data_is_joined_and_a_missing_final_blank_line_is_tolerated() {
        let mut p = SseParser::default();
        assert!(p.feed(b"data: one\ndata: two\n").is_empty());
        assert_eq!(p.finish().unwrap().data, "one\ntwo");
        assert!(p.feed(b"data: tail").is_empty());
        assert_eq!(p.finish().unwrap().data, "tail");
    }
}
