//! Zip archives: read like a suspicious stranger's, written as a clean copy.
//!
//! Reading trusts nothing the archive says about itself:
//! - the end record must sit exactly at the end;
//! - the central directory must end exactly where the end record (or its zip64 twin) begins;
//! - the first file must start at byte 0, with nothing hidden in front (an .exe glued to a zip);
//! - every local header must agree with the directory (a name shown to one reader and another to the next is a trick);
//! - no two files may share bytes (the "overlapping files" bomb);
//! - every file is inflated with its declared size as a hard ceiling and its CRC checked.
//!
//! Only "stored" and "deflate" are accepted: they are what Java's ZipFile, and so Minecraft, can read. No encryption,
//! no symlinks, no archives split over several files.
//!
//! Writing makes the archive the site serves: files in name order, fixed timestamps (the same pack gives the same
//! bytes, so the same hash), UTF-8 names, no extra fields, no comments.

use std::borrow::Cow;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::os::unix::fs::FileExt;

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;

/// Why an archive was refused, in words for its author.
#[derive(Debug, Clone, PartialEq)]
pub struct Bad(pub String);

fn bad<T>(why: &str) -> Result<T, Bad> {
    Err(Bad(why.to_string()))
}

fn io_bad(e: io::Error) -> Bad {
    Bad(format!("the archive ends early or can't be read ({e})"))
}

/// One file of an archive, as its central directory describes it (checked against its local header).
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: Vec<u8>,
    pub method: u16,
    pub flags: u16,
    pub crc: u32,
    pub packed: u64,
    pub size: u64,
    pub local: u64,
    pub data: u64,
    pub symlink: bool,
}

pub struct Limits {
    pub entries: usize,  // files and folders together
    pub directory: u64,  // bytes of the central directory
    pub name: usize,     // bytes of one name
}

const EOCD: u32 = 0x0605_4b50;
const Z64_LOCATOR: u32 = 0x0706_4b50;
const Z64_EOCD: u32 = 0x0606_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn u64le(b: &[u8], at: usize) -> u64 {
    let mut x = [0u8; 8];
    x.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(x)
}

/// The archive's list of files, every claim in it checked.
pub fn directory(f: &File, len: u64, lim: &Limits) -> Result<Vec<Entry>, Bad> {
    if len < 22 {
        return bad("not a zip archive");
    }
    let tail_len = len.min(22 + 65_535) as usize;
    let mut tail = vec![0u8; tail_len];
    f.read_exact_at(&mut tail, len - tail_len as u64).map_err(io_bad)?;

    // The end record: its comment has to run exactly to the last byte of the file.
    let found = (0..=tail_len - 22).rev().find(|&i| u32le(&tail, i) == EOCD && i + 22 + u16le(&tail, i + 20) as usize == tail_len);
    let Some(i) = found else {
        return bad("not a zip archive (or something is glued after its end)");
    };
    let eocd = len - tail_len as u64 + i as u64;
    let (disk, dir_disk) = (u16le(&tail, i + 4), u16le(&tail, i + 6));
    let mut here = u16le(&tail, i + 8) as u64;
    let mut total = u16le(&tail, i + 10) as u64;
    let mut dir_size = u32le(&tail, i + 12) as u64;
    let mut dir_at = u32le(&tail, i + 16) as u64;
    let mut dir_end = eocd;

    if here == 0xFFFF || total == 0xFFFF || dir_size == 0xFFFF_FFFF || dir_at == 0xFFFF_FFFF {
        // Zip64: a locator right before the end record points at the real numbers.
        if eocd < 20 {
            return bad("a broken zip64 archive");
        }
        let mut loc = [0u8; 20];
        f.read_exact_at(&mut loc, eocd - 20).map_err(io_bad)?;
        if u32le(&loc, 0) != Z64_LOCATOR || u32le(&loc, 4) != 0 || u32le(&loc, 16) > 1 {
            return bad("a broken zip64 archive");
        }
        let rec_at = u64le(&loc, 8);
        if rec_at.checked_add(56).map_or(true, |e| e > eocd - 20) {
            return bad("a broken zip64 archive");
        }
        let mut rec = [0u8; 56];
        f.read_exact_at(&mut rec, rec_at).map_err(io_bad)?;
        if u32le(&rec, 0) != Z64_EOCD || rec_at + 12 + u64le(&rec, 4) != eocd - 20 {
            return bad("a broken zip64 archive");
        }
        if u32le(&rec, 16) != 0 || u32le(&rec, 20) != 0 {
            return bad("an archive split over several files");
        }
        here = u64le(&rec, 24);
        total = u64le(&rec, 32);
        dir_size = u64le(&rec, 40);
        dir_at = u64le(&rec, 48);
        dir_end = rec_at;
    } else if disk != 0 || dir_disk != 0 {
        return bad("an archive split over several files");
    }
    if here != total {
        return bad("an archive split over several files");
    }
    if dir_at.checked_add(dir_size) != Some(dir_end) {
        return bad("the archive's list of files is not where it says (something is hidden inside)");
    }
    if dir_size > lim.directory || total as usize > lim.entries {
        return bad(&format!("too many files (the most is {})", lim.entries));
    }

    let mut dir = vec![0u8; dir_size as usize];
    f.read_exact_at(&mut dir, dir_at).map_err(io_bad)?;
    let mut out = Vec::with_capacity(total as usize);
    let mut p = 0usize;
    for _ in 0..total {
        if p + 46 > dir.len() || u32le(&dir, p) != CENTRAL {
            return bad("the archive's list of files is broken");
        }
        let made_by = u16le(&dir, p + 4);
        let flags = u16le(&dir, p + 8);
        let method = u16le(&dir, p + 10);
        let crc = u32le(&dir, p + 16);
        let mut packed = u32le(&dir, p + 20) as u64;
        let mut size = u32le(&dir, p + 24) as u64;
        let (n, m, k) = (u16le(&dir, p + 28) as usize, u16le(&dir, p + 30) as usize, u16le(&dir, p + 32) as usize);
        let mut first_disk = u16le(&dir, p + 34) as u32;
        let external = u32le(&dir, p + 38);
        let mut local = u32le(&dir, p + 42) as u64;
        if p + 46 + n + m + k > dir.len() {
            return bad("the archive's list of files is broken");
        }
        if n == 0 || n > lim.name {
            return bad("a file name that is empty or too long");
        }
        let name = dir[p + 46..p + 46 + n].to_vec();
        let extra = &dir[p + 46 + n..p + 46 + n + m];
        if size == 0xFFFF_FFFF || packed == 0xFFFF_FFFF || local == 0xFFFF_FFFF || first_disk == 0xFFFF {
            // Zip64 sizes: only the fields that overflowed are there, in this order.
            let mut q = 0usize;
            let mut z = None;
            while q + 4 <= extra.len() {
                let (id, len) = (u16le(extra, q), u16le(extra, q + 2) as usize);
                if q + 4 + len > extra.len() {
                    return bad("the archive's list of files is broken");
                }
                if id == 0x0001 {
                    z = Some(&extra[q + 4..q + 4 + len]);
                }
                q += 4 + len;
            }
            let Some(z) = z else {
                return bad("a broken zip64 archive");
            };
            let mut r = 0usize;
            let mut take = |slot: &mut u64| -> Result<(), Bad> {
                if r + 8 > z.len() {
                    return bad("a broken zip64 archive");
                }
                *slot = u64le(z, r);
                r += 8;
                Ok(())
            };
            if size == 0xFFFF_FFFF {
                take(&mut size)?;
            }
            if packed == 0xFFFF_FFFF {
                take(&mut packed)?;
            }
            if local == 0xFFFF_FFFF {
                take(&mut local)?;
            }
            if first_disk == 0xFFFF {
                let mut d = 0u64;
                take(&mut d)?;
                first_disk = d as u32;
            }
        }
        if first_disk != 0 {
            return bad("an archive split over several files");
        }
        let symlink = (made_by >> 8) == 3 && (external >> 16) & 0o170000 == 0o120000;
        out.push(Entry { name, method, flags, crc, packed, size, local, data: 0, symlink });
        p += 46 + n + m + k;
    }
    if p != dir.len() {
        return bad("the archive's list of files has something extra in it");
    }

    // Every local header against the directory.
    for e in out.iter_mut() {
        if e.flags & 0x0001 != 0 || e.flags & 0x0040 != 0 {
            return bad("the archive is encrypted");
        }
        if e.method != 0 && e.method != 8 {
            return bad("the archive is packed with a method Minecraft can't read (make it a plain zip)");
        }
        if e.method == 0 && e.packed != e.size {
            return bad("the archive's list of files is broken");
        }
        let mut h = [0u8; 30];
        if e.local.checked_add(30).map_or(true, |x| x > dir_at) {
            return bad("the archive's list of files points outside it");
        }
        f.read_exact_at(&mut h, e.local).map_err(io_bad)?;
        if u32le(&h, 0) != LOCAL {
            return bad("the archive's list of files points at nothing");
        }
        let (n, m) = (u16le(&h, 26) as u64, u16le(&h, 28) as u64);
        if n != e.name.len() as u64 {
            return bad("a file is named one way in the list and another in its header");
        }
        let mut local_name = vec![0u8; n as usize];
        f.read_exact_at(&mut local_name, e.local + 30).map_err(io_bad)?;
        if local_name != e.name {
            return bad("a file is named one way in the list and another in its header");
        }
        if u16le(&h, 8) != e.method {
            return bad("a file's header disagrees with the list of files");
        }
        if u16le(&h, 6) & 0x0008 == 0 {
            let (crc, packed, size) = (u32le(&h, 14), u32le(&h, 18), u32le(&h, 22));
            if crc != e.crc || (packed != 0xFFFF_FFFF && packed as u64 != e.packed) || (size != 0xFFFF_FFFF && size as u64 != e.size) {
                return bad("a file's header disagrees with the list of files");
            }
        }
        e.data = e.local + 30 + n + m;
    }

    // Nothing in front, no two files sharing bytes, nothing running into the list of files.
    let mut order: Vec<&Entry> = out.iter().collect();
    order.sort_by_key(|e| e.local);
    if let Some(first) = order.first() {
        if first.local != 0 {
            return bad("something is hidden in front of the archive");
        }
    }
    for (i, e) in order.iter().enumerate() {
        let end = e.data.checked_add(e.packed).ok_or_else(|| Bad("a file is bigger than the archive".into()))?;
        let limit = order.get(i + 1).map_or(dir_at, |next| next.local);
        if end > limit {
            return bad("two files in the archive share the same bytes (a zip bomb)");
        }
    }
    Ok(out)
}

/// Bytes `at..end` of the file, read in order.
struct Span<'a> {
    f: &'a File,
    at: u64,
    end: u64,
}

impl Read for Span<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = (buf.len() as u64).min(self.end - self.at) as usize;
        if n == 0 {
            return Ok(0);
        }
        let got = self.f.read_at(&mut buf[..n], self.at)?;
        self.at += got as u64;
        Ok(got)
    }
}

/// A file of the archive, inflated: never more than it says, exactly as much as it says, with the right CRC.
pub fn read(f: &File, e: &Entry) -> Result<Vec<u8>, Bad> {
    let span = Span { f, at: e.data, end: e.data + e.packed };
    let mut inner: Box<dyn Read> = if e.method == 8 { Box::new(DeflateDecoder::new(BufReader::with_capacity(64 * 1024, span))) } else { Box::new(span) };
    let mut out = Vec::with_capacity(e.size.min(64 * 1024 * 1024) as usize);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        // Asking for one byte more than is left tells a file that grows past its size.
        let room = (e.size - out.len() as u64 + 1).min(buf.len() as u64) as usize;
        let n = inner.read(&mut buf[..room]).map_err(|_| Bad("a file in the archive is damaged".into()))?;
        if n == 0 {
            break;
        }
        if out.len() as u64 + n as u64 > e.size {
            return bad("a file inflates to more than it says (a zip bomb)");
        }
        out.extend_from_slice(&buf[..n]);
    }
    if out.len() as u64 != e.size {
        return bad("a file in the archive is shorter than it says");
    }
    if crc32fast::hash(&out) != e.crc {
        return bad("a file in the archive is damaged (its checksum doesn't match)");
    }
    Ok(out)
}

/// The clean archive, written as it goes (a pack may be a gigabyte unpacked: it is never all in memory). The caller
/// adds files in name order, so the same pack always comes out as the same bytes.
pub struct Writer {
    out: BufWriter<File>,
    at: u64,
    dir: Vec<u8>,
    count: usize,
    last: String,
}

// 1 January 2026, 00:00, in MS-DOS terms: every file gets the same moment.
const DOS_DATE: u16 = ((2026 - 1980) << 9) | (1 << 5) | 1;
const DOS_TIME: u16 = 0;

impl Writer {
    pub fn new(to: File) -> Self {
        Writer { out: BufWriter::with_capacity(1 << 20, to), at: 0, dir: Vec::new(), count: 0, last: String::new() }
    }

    /// `squeeze`: deflate it (text, models); PNG and Ogg are compressed already and go as they are.
    pub fn add(&mut self, name: &str, data: &[u8], squeeze: bool) -> io::Result<()> {
        assert!(self.count == 0 || name > self.last.as_str(), "files go in name order");
        assert!(self.count + 1 < 0xFFFF, "no zip64 here");
        let crc = crc32fast::hash(data);
        let (method, body): (u16, Cow<[u8]>) = if squeeze && data.len() > 64 {
            let mut enc = DeflateEncoder::new(Vec::with_capacity(data.len() / 2), Compression::new(6));
            enc.write_all(data)?;
            let packed = enc.finish()?;
            if packed.len() < data.len() { (8, Cow::Owned(packed)) } else { (0, Cow::Borrowed(data)) }
        } else {
            (0, Cow::Borrowed(data))
        };
        assert!(self.at < u32::MAX as u64 && (data.len() as u64) < u32::MAX as u64, "no zip64 here");
        let n = name.len() as u16;
        let mut local = Vec::with_capacity(30 + name.len());
        local.extend_from_slice(&LOCAL.to_le_bytes());
        local.extend_from_slice(&20u16.to_le_bytes()); // version needed: 2.0
        local.extend_from_slice(&0x0800u16.to_le_bytes()); // names in UTF-8
        local.extend_from_slice(&method.to_le_bytes());
        local.extend_from_slice(&DOS_TIME.to_le_bytes());
        local.extend_from_slice(&DOS_DATE.to_le_bytes());
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&(body.len() as u32).to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&n.to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name.as_bytes());
        self.out.write_all(&local)?;
        self.out.write_all(&body)?;

        let d = &mut self.dir;
        d.extend_from_slice(&CENTRAL.to_le_bytes());
        d.extend_from_slice(&20u16.to_le_bytes()); // made by: MS-DOS, 2.0
        d.extend_from_slice(&20u16.to_le_bytes());
        d.extend_from_slice(&0x0800u16.to_le_bytes());
        d.extend_from_slice(&method.to_le_bytes());
        d.extend_from_slice(&DOS_TIME.to_le_bytes());
        d.extend_from_slice(&DOS_DATE.to_le_bytes());
        d.extend_from_slice(&crc.to_le_bytes());
        d.extend_from_slice(&(body.len() as u32).to_le_bytes());
        d.extend_from_slice(&(data.len() as u32).to_le_bytes());
        d.extend_from_slice(&n.to_le_bytes());
        d.extend_from_slice(&[0u8; 12]); // extra, comment, disk, internal attributes: none
        d.extend_from_slice(&(self.at as u32).to_le_bytes());
        d.extend_from_slice(name.as_bytes());
        self.at += local.len() as u64 + body.len() as u64;
        self.count += 1;
        self.last = name.to_string();
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// The list of files and the end record; the archive's size in bytes.
    pub fn finish(mut self) -> io::Result<u64> {
        assert!(self.at + (self.dir.len() as u64) < u32::MAX as u64, "no zip64 here");
        let count = self.count as u16;
        self.out.write_all(&self.dir)?;
        let mut end = Vec::with_capacity(22);
        end.extend_from_slice(&EOCD.to_le_bytes());
        end.extend_from_slice(&[0u8; 4]);
        end.extend_from_slice(&count.to_le_bytes());
        end.extend_from_slice(&count.to_le_bytes());
        end.extend_from_slice(&(self.dir.len() as u32).to_le_bytes());
        end.extend_from_slice(&(self.at as u32).to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        self.out.write_all(&end)?;
        self.out.flush()?;
        self.out.get_ref().sync_all()?;
        Ok(self.at + self.dir.len() as u64 + 22)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Seek;

    pub const LIMITS: Limits = Limits { entries: 1000, directory: 1 << 20, name: 512 };

    /// A zip written by hand, so broken ones can be made on purpose.
    pub struct Raw {
        pub bytes: Vec<u8>,
        dir: Vec<u8>,
        count: u16,
    }

    impl Raw {
        pub fn new() -> Self {
            Raw { bytes: Vec::new(), dir: Vec::new(), count: 0 }
        }

        /// A file; `central_name` differing from `name` makes the header lie; `size_claim` makes the sizes lie.
        pub fn file(&mut self, name: &str, data: &[u8], deflate: bool, central_name: Option<&str>, size_claim: Option<u32>, made_by: u16, external: u32) -> &mut Self {
            let body = if deflate {
                let mut enc = DeflateEncoder::new(Vec::new(), Compression::new(6));
                enc.write_all(data).unwrap();
                enc.finish().unwrap()
            } else {
                data.to_vec()
            };
            let method: u16 = if deflate { 8 } else { 0 };
            let crc = crc32fast::hash(data);
            let size = size_claim.unwrap_or(data.len() as u32);
            let at = self.bytes.len() as u32;
            let mut h = Vec::new();
            h.extend_from_slice(&LOCAL.to_le_bytes());
            h.extend_from_slice(&20u16.to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            h.extend_from_slice(&method.to_le_bytes());
            h.extend_from_slice(&[0u8; 4]);
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(body.len() as u32).to_le_bytes());
            h.extend_from_slice(&size.to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            h.extend_from_slice(name.as_bytes());
            self.bytes.extend_from_slice(&h);
            self.bytes.extend_from_slice(&body);
            self.central(central_name.unwrap_or(name), method, crc, body.len() as u32, size, at, made_by, external);
            self
        }

        pub fn central(&mut self, name: &str, method: u16, crc: u32, packed: u32, size: u32, at: u32, made_by: u16, external: u32) -> &mut Self {
            let d = &mut self.dir;
            d.extend_from_slice(&CENTRAL.to_le_bytes());
            d.extend_from_slice(&made_by.to_le_bytes());
            d.extend_from_slice(&20u16.to_le_bytes());
            d.extend_from_slice(&0u16.to_le_bytes());
            d.extend_from_slice(&method.to_le_bytes());
            d.extend_from_slice(&[0u8; 4]);
            d.extend_from_slice(&crc.to_le_bytes());
            d.extend_from_slice(&packed.to_le_bytes());
            d.extend_from_slice(&size.to_le_bytes());
            d.extend_from_slice(&(name.len() as u16).to_le_bytes());
            d.extend_from_slice(&[0u8; 8]);
            d.extend_from_slice(&external.to_le_bytes());
            d.extend_from_slice(&at.to_le_bytes());
            d.extend_from_slice(name.as_bytes());
            self.count += 1;
            self
        }

        pub fn finish(&mut self) -> Vec<u8> {
            let at = self.bytes.len() as u32;
            let mut b = self.bytes.clone();
            b.extend_from_slice(&self.dir);
            b.extend_from_slice(&EOCD.to_le_bytes());
            b.extend_from_slice(&[0u8; 4]);
            b.extend_from_slice(&self.count.to_le_bytes());
            b.extend_from_slice(&self.count.to_le_bytes());
            b.extend_from_slice(&(self.dir.len() as u32).to_le_bytes());
            b.extend_from_slice(&at.to_le_bytes());
            b.extend_from_slice(&0u16.to_le_bytes());
            b
        }
    }

    pub fn temp(bytes: &[u8]) -> File {
        let path = std::env::temp_dir().join(format!("cursed-zip-{}-{}", std::process::id(), rand_core::RngCore::next_u64(&mut rand_core::OsRng)));
        let mut f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        f.write_all(bytes).unwrap();
        f.rewind().unwrap();
        f
    }

    fn check(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, Bad> {
        let f = temp(bytes);
        let list = directory(&f, bytes.len() as u64, &LIMITS)?;
        list.iter().map(|e| Ok((String::from_utf8_lossy(&e.name).into_owned(), read(&f, e)?))).collect()
    }

    #[test]
    fn a_plain_archive_reads_back() {
        let z = Raw::new().file("pack.mcmeta", b"{\"pack\":{}}", false, None, None, 20, 0).file("assets/a.txt", &b"hello ".repeat(100), true, None, None, 20, 0).finish();
        let files = check(&z).unwrap();
        assert_eq!(files[0].0, "pack.mcmeta");
        assert_eq!(files[1].1.len(), 600);
    }

    #[test]
    fn the_written_archive_reads_back_and_is_the_same_every_time() {
        let make = || {
            let f = temp(&[]);
            let mut w = Writer::new(f.try_clone().unwrap());
            w.add("a/one.png", &[7u8; 100], false).unwrap();
            w.add("b/two.json", &b"{\"a\": [1, 2, 3]}".repeat(20), true).unwrap();
            w.finish().unwrap();
            let mut bytes = Vec::new();
            let mut g = f;
            g.rewind().unwrap();
            g.read_to_end(&mut bytes).unwrap();
            bytes
        };
        let (a, b) = (make(), make());
        assert_eq!(a, b);
        let files = check(&a).unwrap();
        assert_eq!(files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["a/one.png", "b/two.json"]);
        assert_eq!(files[1].1.len(), 320);
    }

    #[test]
    fn a_lie_about_the_size_is_a_bomb() {
        let z = Raw::new().file("big.txt", &vec![b'a'; 100_000], true, None, Some(1000), 20, 0).finish();
        assert!(check(&z).unwrap_err().0.contains("more than it says"));
    }

    #[test]
    fn two_files_sharing_bytes_are_a_bomb() {
        // The overlapping-files bomb: the second file's header and data sit inside the first file's data.
        let mut inner = Raw::new();
        inner.file("b.txt", b"hello", false, None, None, 20, 0);
        let mut r = Raw::new();
        r.file("a.txt", &inner.bytes.clone(), false, None, None, 20, 0);
        r.central("b.txt", 0, crc32fast::hash(b"hello"), 5, 5, 35, 20, 0);
        assert!(check(&r.finish()).unwrap_err().0.contains("share"));
    }

    #[test]
    fn a_name_that_differs_between_header_and_list_is_refused() {
        let z = Raw::new().file("pack.png", b"x", false, Some("pack.exe"), None, 20, 0).finish();
        assert!(check(&z).unwrap_err().0.contains("named one way"));
    }

    #[test]
    fn something_in_front_is_refused() {
        let mut z = b"MZ\x90\x00 this is not a zip".to_vec();
        let inner = Raw::new().file("a.txt", b"hi", false, None, None, 20, 0).finish();
        // Shift the offsets as a self-extractor would.
        let mut fixed = Raw::new();
        fixed.bytes = z.clone();
        fixed.file("a.txt", b"hi", false, None, None, 20, 0);
        z = fixed.finish();
        assert!(check(&z).unwrap_err().0.contains("in front"));
        assert!(check(&inner).is_ok());
    }

    #[test]
    fn something_after_is_refused() {
        let mut z = Raw::new().file("a.txt", b"hi", false, None, None, 20, 0).finish();
        z.extend_from_slice(b"tail");
        assert!(check(&z).is_err());
    }

    #[test]
    fn a_damaged_file_is_found() {
        let mut z = Raw::new().file("a.txt", b"hello world", false, None, None, 20, 0).finish();
        z[36] ^= 0xFF; // a byte of the data (the header is 30 bytes and the name 5)
        assert!(check(&z).unwrap_err().0.contains("damaged"));
    }

    #[test]
    fn symlinks_are_marked() {
        let z = Raw::new().file("link", b"/etc/passwd", false, None, None, 0x031E, 0o120777 << 16).finish();
        let f = temp(&z);
        assert!(directory(&f, z.len() as u64, &LIMITS).unwrap()[0].symlink);
    }

    #[test]
    fn not_a_zip() {
        assert!(check(b"hello, this is text and not a zip archive at all").is_err());
        assert!(check(b"").is_err());
    }
}
