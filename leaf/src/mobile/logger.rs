use std::{
    ffi,
    io::{self, Write},
};

use bytes::BytesMut;

#[cfg(any(target_os = "ios", target_os = "macos"))]
use super::bindings::{asl_log, ASL_LEVEL_NOTICE};

#[cfg(target_os = "android")]
use super::bindings::{__android_log_print, android_LogPriority_ANDROID_LOG_VERBOSE};

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn log_out(data: &[u8]) {
    unsafe {
        let s = match ffi::CString::new(data) {
            Ok(s) => s,
            Err(_) => return,
        };
        asl_log(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            ASL_LEVEL_NOTICE as i32,
            s.as_c_str().as_ptr(),
        )
    };
}

#[cfg(target_os = "android")]
fn log_out(data: &[u8]) {
    if data.is_empty() {
        return;
    }
    unsafe {
        let s = match ffi::CString::new(data) {
            Ok(s) => s,
            Err(_) => return,
        };
        let tag = ffi::CString::new("leaf").unwrap();
        let _ = __android_log_print(
            android_LogPriority_ANDROID_LOG_VERBOSE as std::os::raw::c_int,
            tag.as_c_str().as_ptr(),
            s.as_c_str().as_ptr(),
        );
    }
}

#[cfg(target_os = "windows")]
pub fn log_text(text: &str) {
    extern "system" {
        fn OutputDebugStringW(lp_output_string: *const u16);
    }
    use std::{ffi::OsStr, os::windows::prelude::OsStrExt};

    let mut bytes: Vec<_> = OsStr::new(text).encode_wide().collect();
    bytes.push(0);
    unsafe { OutputDebugStringW(bytes.as_ptr()) };
}

#[derive(Debug)]
pub struct ConsoleWriter(pub BytesMut);

impl Default for ConsoleWriter {
    fn default() -> Self {
        ConsoleWriter(BytesMut::new())
    }
}

unsafe impl Send for ConsoleWriter {}

impl Write for ConsoleWriter {
    #[cfg(not(target_os = "windows"))]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(buf);
        if let Some(i) = memchr::memchr(b'\n', &self.0) {
            log_out(&self.0[..i]);
            let _ = self.0.split_to(i + 1);
        }
        Ok(buf.len())
    }

    #[cfg(target_os = "windows")]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        log_text(String::from_utf8_lossy(buf).as_ref());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
