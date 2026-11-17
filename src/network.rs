//! Bounded HTTPS GET with normal Windows TLS validation and no redirects.
use std::sync::atomic::{AtomicBool, Ordering};
use windows::{Win32::Networking::WinHttp::*, core::*};
struct Internet(*mut std::ffi::c_void);
impl Internet {
    fn checked(h: *mut std::ffi::c_void) -> Result<Self> {
        if h.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self(h))
        }
    }
}
impl Drop for Internet {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}
pub struct Response {
    pub status: u32,
    pub body: Vec<u8>,
    pub retry_after: u64,
}
pub fn get(host: &str, path: &str, stop: &AtomicBool) -> Result<Response> {
    unsafe {
        let session = Internet::checked(WinHttpOpen(
            w!("DynamicIslandForWindows/0.6.0 (Windows desktop weather)"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            None,
            None,
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 1000, 1800, 1200, 1800)?;
        let connection =
            Internet::checked(WinHttpConnect(session.0, &HSTRING::from(host), 443, 0))?;
        let request = Internet::checked(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            &HSTRING::from(path),
            None,
            None,
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes()),
        )?;
        if stop.load(Ordering::Relaxed) {
            return Err(Error::empty());
        }
        WinHttpSendRequest(request.0, None, None, 0, 0, 0)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())?;
        let mut status = 0u32;
        let mut len = 4;
        let mut index = 0;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            None,
            Some((&mut status as *mut u32).cast()),
            &mut len,
            &mut index,
        )?;
        let mut retry = [0u16; 128];
        let mut len = 256;
        let mut index = 0;
        let retry_after = if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_RETRY_AFTER,
            None,
            Some(retry.as_mut_ptr().cast()),
            &mut len,
            &mut index,
        )
        .is_ok()
        {
            String::from_utf16_lossy(
                &retry[..retry.iter().position(|c| *c == 0).unwrap_or(retry.len())],
            )
            .trim()
            .parse()
            .unwrap_or(3600)
        } else {
            60
        };
        let mut body = Vec::new();
        let mut buffer = [0u8; 8192];
        while status == 200 && !stop.load(Ordering::Relaxed) {
            let mut read = 0;
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )?;
            if read == 0 {
                break;
            }
            if body.len() + read as usize > 1_048_576 {
                return Err(Error::new(
                    HRESULT(0x80070057u32 as i32),
                    "Response too large",
                ));
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        Ok(Response {
            status,
            body,
            retry_after,
        })
    }
}
