use std::io::Write;
use std::process::{Command, Stdio};

/// How text was delivered to the user's clipboard pathway.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardMethod {
    Native,
    Osc52,
}

/// Result of attempting to write the Selection to a clipboard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardResult {
    Copied(ClipboardMethod),
    Failed(String),
}

/// Abstraction over clipboard backends so tests never touch the real clipboard.
pub trait ClipboardWriter {
    fn write_text(&mut self, text: &str) -> ClipboardResult;
}

/// Production adapter: native macOS/Linux tools, then OSC 52 fallback.
pub struct SystemClipboard {
    osc52: Option<Box<dyn Write + Send>>,
}

impl SystemClipboard {
    #[must_use]
    pub fn new() -> Self {
        Self { osc52: None }
    }

    /// Attach a writer used for OSC 52 fallback (normally the terminal).
    #[must_use]
    pub fn with_osc52_writer(writer: impl Write + Send + 'static) -> Self {
        Self {
            osc52: Some(Box::new(writer)),
        }
    }

    fn try_native(text: &str) -> Result<(), String> {
        if try_command_with_stdin("pbcopy", &[], text) {
            return Ok(());
        }
        if try_command_with_stdin("wl-copy", &[], text) {
            return Ok(());
        }
        if try_command_with_stdin("xclip", &["-selection", "clipboard"], text) {
            return Ok(());
        }
        if try_command_with_stdin("xsel", &["--clipboard", "--input"], text) {
            return Ok(());
        }
        Err("no native clipboard command available".to_owned())
    }

    fn try_osc52(&mut self, text: &str) -> Result<(), String> {
        let Some(writer) = self.osc52.as_mut() else {
            return Err("OSC 52 fallback is unavailable".to_owned());
        };
        let payload = encode_osc52(text);
        writer
            .write_all(payload.as_bytes())
            .and_then(|()| writer.flush())
            .map_err(|error| format!("OSC 52 write failed: {error}"))
    }
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardWriter for SystemClipboard {
    fn write_text(&mut self, text: &str) -> ClipboardResult {
        copy_with_fallback(text, Self::try_native, |payload| self.try_osc52(payload))
    }
}

/// Test double that records writes and returns a configured result.
#[derive(Debug)]
pub struct FakeClipboard {
    pub writes: Vec<String>,
    pub result: ClipboardResult,
}

impl FakeClipboard {
    #[must_use]
    pub fn succeeding() -> Self {
        Self {
            writes: Vec::new(),
            result: ClipboardResult::Copied(ClipboardMethod::Native),
        }
    }

    #[must_use]
    pub fn failing(message: impl Into<String>) -> Self {
        Self {
            writes: Vec::new(),
            result: ClipboardResult::Failed(message.into()),
        }
    }
}

impl ClipboardWriter for FakeClipboard {
    fn write_text(&mut self, text: &str) -> ClipboardResult {
        self.writes.push(text.to_owned());
        self.result.clone()
    }
}

/// Clipboard decision logic with injectable backends for adapter tests.
pub struct ClipboardAdapter<N, O> {
    native: N,
    osc52: O,
}

impl<N, O> ClipboardAdapter<N, O>
where
    N: FnMut(&str) -> Result<(), String>,
    O: FnMut(&str) -> Result<(), String>,
{
    pub fn new(native: N, osc52: O) -> Self {
        Self { native, osc52 }
    }

    pub fn copy(&mut self, text: &str) -> ClipboardResult {
        copy_with_fallback(text, &mut self.native, &mut self.osc52)
    }
}

fn copy_with_fallback(
    text: &str,
    mut native: impl FnMut(&str) -> Result<(), String>,
    mut osc52: impl FnMut(&str) -> Result<(), String>,
) -> ClipboardResult {
    match native(text) {
        Ok(()) => ClipboardResult::Copied(ClipboardMethod::Native),
        Err(native_error) => match osc52(text) {
            Ok(()) => ClipboardResult::Copied(ClipboardMethod::Osc52),
            Err(osc_error) => ClipboardResult::Failed(format!("{native_error}; {osc_error}")),
        },
    }
}

pub fn encode_osc52(text: &str) -> String {
    let encoded = base64_encode(text.as_bytes());
    format!("\x1b]52;c;{encoded}\x07")
}

fn try_command_with_stdin(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return false;
    };
    if stdin.write_all(text.as_bytes()).is_err() {
        let _ = child.kill();
        return false;
    }
    drop(stdin);
    matches!(child.wait(), Ok(status) if status.success())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        output.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(triple & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_prefers_native_and_falls_back_to_osc52() {
        let mut native_calls = 0;
        let mut osc_calls = 0;
        let mut adapter = ClipboardAdapter::new(
            |text| {
                native_calls += 1;
                assert_eq!(text, "hello");
                Err("native blocked".into())
            },
            |text| {
                osc_calls += 1;
                assert_eq!(text, "hello");
                Ok(())
            },
        );

        assert_eq!(
            adapter.copy("hello"),
            ClipboardResult::Copied(ClipboardMethod::Osc52)
        );
        assert_eq!(native_calls, 1);
        assert_eq!(osc_calls, 1);

        let mut adapter = ClipboardAdapter::new(|_| Ok(()), |_| panic!("osc should not run"));
        assert_eq!(
            adapter.copy("native"),
            ClipboardResult::Copied(ClipboardMethod::Native)
        );
    }

    #[test]
    fn adapter_reports_combined_failure() {
        let mut adapter =
            ClipboardAdapter::new(|_| Err("no pbcopy".into()), |_| Err("no tty".into()));
        assert_eq!(
            adapter.copy("x"),
            ClipboardResult::Failed("no pbcopy; no tty".into())
        );
    }

    #[test]
    fn osc52_payload_is_base64_wrapped() {
        let payload = encode_osc52("Hi");
        assert!(payload.starts_with("\x1b]52;c;"));
        assert!(payload.ends_with('\x07'));
        assert!(payload.contains("SGk="));
    }
}
