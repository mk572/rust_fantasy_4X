//! Getting a file from the Internet (`ureq`, TLS by rustls). Only the ROM download uses it, and only
//! after the player agreed (`docs/installer-design.md` 3.2). Nothing about the player is sent.

use std::io::Read;

use f4x_setup::roms::Fetch;

/// The real download.
pub struct Http;

impl Fetch for Http {
    fn get(&self, url: &str, limit: u64, progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>, String> {
        let response = ureq::get(url).call().map_err(|e| format!("{url}: {e}"))?;
        let mut reader = response.into_body().into_reader();
        let mut bytes = Vec::new();
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut chunk).map_err(|e| format!("{url}: {e}"))?;
            if n == 0 {
                return Ok(bytes);
            }
            bytes.extend_from_slice(&chunk[..n]);
            if bytes.len() as u64 > limit {
                return Err(format!("{url}: more than {limit} bytes"));
            }
            progress(bytes.len() as u64);
        }
    }
}
