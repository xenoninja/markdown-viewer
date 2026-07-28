use std::fs::File;
use std::io::{ErrorKind, Read};

use nix::libc;

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

pub fn contains_rendered_text(output: &[u8], expected: &[u8]) -> bool {
    let mut visible = Vec::with_capacity(output.len());
    let mut index = 0;
    while index < output.len() {
        if output[index] == b'\x1b' && output.get(index + 1) == Some(&b'[') {
            index += 2;
            while index < output.len() {
                let byte = output[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        } else {
            visible.push(output[index]);
            index += 1;
        }
    }
    contains(&visible, expected)
}

pub fn read_available(terminal: &mut File, output: &mut Vec<u8>) {
    loop {
        let mut chunk = [0_u8; 4096];
        match terminal.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => output.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) => panic!("read PTY output: {error}"),
        }
    }
}
