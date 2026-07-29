use std::process::{Command, Stdio};

/// Result of attempting to open a URL in the system browser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserResult {
    Opened,
    Failed(String),
}

/// Abstraction over browser launchers so tests never open a real browser.
pub trait BrowserLauncher {
    fn open_url(&mut self, url: &str) -> BrowserResult;
}

/// Production adapter: platform open helpers for explicit `http`/`https` only.
pub struct SystemBrowser;

impl SystemBrowser {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemBrowser {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserLauncher for SystemBrowser {
    fn open_url(&mut self, url: &str) -> BrowserResult {
        if !is_web_url(url) {
            return BrowserResult::Failed("unsupported URL scheme".to_owned());
        }
        if open_with_command("open", url) || open_with_command("xdg-open", url) {
            BrowserResult::Opened
        } else {
            BrowserResult::Failed("no system browser launcher available".to_owned())
        }
    }
}

fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Test double that records requests and returns a configured result.
#[derive(Clone, Debug)]
pub struct FakeBrowser {
    pub result: BrowserResult,
    pub opened: Vec<String>,
}

impl FakeBrowser {
    #[must_use]
    pub fn succeeding() -> Self {
        Self {
            result: BrowserResult::Opened,
            opened: Vec::new(),
        }
    }

    #[must_use]
    pub fn failing(message: impl Into<String>) -> Self {
        Self {
            result: BrowserResult::Failed(message.into()),
            opened: Vec::new(),
        }
    }
}

impl Default for FakeBrowser {
    fn default() -> Self {
        Self::succeeding()
    }
}

impl BrowserLauncher for FakeBrowser {
    fn open_url(&mut self, url: &str) -> BrowserResult {
        self.opened.push(url.to_owned());
        self.result.clone()
    }
}

fn open_with_command(program: &str, url: &str) -> bool {
    Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{BrowserLauncher, BrowserResult, SystemBrowser};

    #[test]
    fn system_browser_rejects_non_web_schemes_without_launching() {
        let mut browser = SystemBrowser::new();
        for url in [
            "./other.md",
            "../readme.md",
            "mailto:a@b.c",
            "javascript:alert(1)",
            "file:///tmp/x",
            "ftp://example.com",
        ] {
            assert_eq!(
                browser.open_url(url),
                BrowserResult::Failed("unsupported URL scheme".to_owned()),
                "{url}"
            );
        }
    }

    #[test]
    fn system_browser_accepts_http_and_https_schemes_case_insensitively() {
        assert!(super::is_web_url("http://example.com"));
        assert!(super::is_web_url("https://example.com"));
        assert!(super::is_web_url("HTTP://EXAMPLE.COM"));
        assert!(super::is_web_url("HTTPS://EXAMPLE.COM/path"));
        assert!(!super::is_web_url("./relative.md"));
        assert!(!super::is_web_url("mailto:a@b.c"));
    }
}
