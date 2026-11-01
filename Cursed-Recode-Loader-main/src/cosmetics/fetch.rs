//! A file off our server over HTTPS (WinHTTP: the system's own certificates and proxy): one session and connection a
//! worker, a request a file.

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Networking::WinHttp::*;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct Client {
    session: *mut core::ffi::c_void,
    connect: *mut core::ffi::c_void,
}

// (the handles are used by the one thread that owns the client)
unsafe impl Send for Client {}

impl Client {
    pub fn new(host: &str) -> Option<Client> {
        unsafe {
            let agent = wide("Cursed Recode");
            let session = WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, std::ptr::null(), std::ptr::null(), 0);
            if session.is_null() {
                return None;
            }
            // a slow line is waited for, a dead one not forever; text comes compressed
            WinHttpSetTimeouts(session, 10_000, 10_000, 30_000, 60_000);
            let decompress: u32 = WINHTTP_DECOMPRESSION_FLAG_GZIP | WINHTTP_DECOMPRESSION_FLAG_DEFLATE;
            WinHttpSetOption(session, WINHTTP_OPTION_DECOMPRESSION, &decompress as *const u32 as *const _, 4);
            let h = wide(host);
            let connect = WinHttpConnect(session, h.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT as u16, 0);
            if connect.is_null() {
                WinHttpCloseHandle(session);
                return None;
            }
            Some(Client { session, connect })
        }
    }

    /// The whole body of `path` (200 only).
    pub fn get(&self, path: &str) -> Result<Vec<u8>, String> {
        unsafe {
            let verb = wide("GET");
            let p = wide(path);
            let req = WinHttpOpenRequest(self.connect, verb.as_ptr(), p.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), WINHTTP_FLAG_SECURE);
            if req.is_null() {
                return Err(format!("open request: {}", GetLastError()));
            }
            let out = (|| {
                if WinHttpSendRequest(req, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) == 0 {
                    return Err(format!("send: {}", GetLastError()));
                }
                if WinHttpReceiveResponse(req, std::ptr::null_mut()) == 0 {
                    return Err(format!("receive: {}", GetLastError()));
                }
                let mut status: u32 = 0;
                let mut len = std::mem::size_of::<u32>() as u32;
                if WinHttpQueryHeaders(req, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, std::ptr::null(), &mut status as *mut u32 as *mut _, &mut len, std::ptr::null_mut()) == 0 {
                    return Err(format!("status: {}", GetLastError()));
                }
                if status != 200 {
                    return Err(format!("HTTP {status}"));
                }
                let mut body = Vec::new();
                loop {
                    let mut avail: u32 = 0;
                    if WinHttpQueryDataAvailable(req, &mut avail) == 0 {
                        return Err(format!("data: {}", GetLastError()));
                    }
                    if avail == 0 {
                        break;
                    }
                    let at = body.len();
                    body.resize(at + avail as usize, 0);
                    let mut got: u32 = 0;
                    if WinHttpReadData(req, body[at..].as_mut_ptr() as *mut _, avail, &mut got) == 0 {
                        return Err(format!("read: {}", GetLastError()));
                    }
                    body.truncate(at + got as usize);
                }
                Ok(body)
            })();
            WinHttpCloseHandle(req);
            out
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.connect);
            WinHttpCloseHandle(self.session);
        }
    }
}
