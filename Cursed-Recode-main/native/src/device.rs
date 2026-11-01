//! This computer, as the beta knows it: a few of its parts, each hashed. The loader sends them when it signs in and
//! starts; the game sends them when it confirms the loader's ticket. The server binds an account to one computer and
//! a computer to one account, and calls two computers the same when most of the parts they both have agree, so a new
//! disk or a reinstalled Windows does not make it a stranger.
//!
//! Parts: the Windows installation (MachineGuid), the board and the system's UUID (from SMBIOS), the processor
//! (CPUID), the system disk's serial, the system volume's serial, and for Recode the TPM's own key (its endorsement
//! key, made into the chip at the factory: no program changes it), the video card's own id, the monitor's serial and
//! Windows' key in the firmware. What a part says is never sent, only its hash.
//! The same as Cursed's (its native/src/device.rs), the last four added. Since 20.10 Recode plays without a TPM too
//! (her word: players on Windows 10 have none, or it is off): the last three are what a common spoofer seldom
//! reaches, and two of them the same make the same computer for the server (beta.rs Parts::same).

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::{CloseHandle, FreeLibrary, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, GetVolumeInformationW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;
use windows_sys::Win32::System::IO::DeviceIoControl;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The parts, named, each a SHA-256 in hex: what the server is given. Read once a run (the game says them every
/// minute; a computer does not change under a running game).
pub fn parts() -> BTreeMap<String, String> {
    static PARTS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    PARTS.get_or_init(read).clone()
}

fn read() -> BTreeMap<String, String> {
    let mut raw: Vec<(&str, Option<String>)> = vec![("win", machine_guid()), ("cpu", cpu())];
    let (board, uuid) = unsafe { smbios() }.unwrap_or((None, None));
    raw.push(("board", board));
    raw.push(("uuid", uuid));
    raw.push(("disk", unsafe { system_disk_serial() }));
    raw.push(("vol", unsafe { system_volume_serial() }));
    // tests (a build from the sources): a computer without a TPM, as a player's on Windows 10 (CURSED_DEV_NO_TPM=1)
    let no_tpm = !crate::license::RELEASE && std::env::var_os("CURSED_DEV_NO_TPM").is_some();
    raw.push(("tpm", if no_tpm { None } else { unsafe { tpm() } }));
    raw.push(("gpu", unsafe { gpu() }));
    raw.push(("monitor", unsafe { monitor() }));
    raw.push(("msdm", unsafe { msdm() }));
    let mut out = BTreeMap::new();
    for (key, value) in raw {
        if let Some(value) = value.as_deref().map(str::trim).filter(|v| meaningful(v)) {
            out.insert(key.to_string(), hash(key, value));
        }
    }
    out
}

/// The parts as JSON, the way the requests carry them.
pub fn parts_json() -> serde_json::Value {
    serde_json::Value::Object(parts().into_iter().map(|(k, v)| (k, serde_json::Value::String(v))).collect())
}

pub fn hash(key: &str, value: &str) -> String {
    let mut h = Sha256::new();
    h.update(crate::vault::device_kdf().as_bytes());
    h.update(key.as_bytes());
    h.update(b"|");
    h.update(value.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// What the makers put where they had nothing to say is not a part of anything.
pub fn meaningful(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    if v.len() < 3 {
        return false;
    }
    let junk = ["to be filled by o.e.m.", "default string", "none", "not applicable", "not specified", "system serial number", "serial number", "o.e.m.", "oem", "123456789", "base board serial number"];
    if junk.contains(&v.as_str()) {
        return false;
    }
    // All the same character: 0000…, FFFF…, 1111….
    let bare: Vec<char> = v.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    !(bare.is_empty() || bare.iter().all(|c| *c == bare[0]))
}

fn machine_guid() -> Option<String> {
    let mut buf = [0u16; 64];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            wide("SOFTWARE\\Microsoft\\Cryptography").as_ptr(),
            wide("MachineGuid").as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    (status == 0).then(|| String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]))
}

fn cpu() -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid;
        let v = __cpuid(0);
        let vendor: Vec<u8> = [v.ebx, v.edx, v.ecx].iter().flat_map(|r| r.to_le_bytes()).collect();
        let signature = __cpuid(1).eax;
        let mut brand = Vec::new();
        if __cpuid(0x8000_0000).eax >= 0x8000_0004 {
            for leaf in 0x8000_0002u32..=0x8000_0004 {
                let r = __cpuid(leaf);
                for reg in [r.eax, r.ebx, r.ecx, r.edx] {
                    brand.extend_from_slice(&reg.to_le_bytes());
                }
            }
        }
        let brand = String::from_utf8_lossy(&brand).trim_matches(char::from(0)).trim().to_string();
        return Some(format!("{}|{signature:08x}|{brand}", String::from_utf8_lossy(&vendor)));
    }
    #[allow(unreachable_code)]
    None
}

/// The board (maker, model, serial) and the system's UUID, out of the SMBIOS tables the firmware gives Windows.
unsafe fn smbios() -> Option<(Option<String>, Option<String>)> {
    const RSMB: u32 = u32::from_be_bytes(*b"RSMB");
    let size = GetSystemFirmwareTable(RSMB, 0, std::ptr::null_mut(), 0);
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    if GetSystemFirmwareTable(RSMB, 0, buf.as_mut_ptr(), size) != size {
        return None;
    }
    // RawSMBIOSData: four version bytes, the length, then the structures.
    if buf.len() < 8 {
        return None;
    }
    let length = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    Some(parse_smbios(&buf[8..(8 + length).min(buf.len())]))
}

/// Walks the SMBIOS structures: type 1 (the system: its UUID) and type 2 (the board: maker, model, serial).
pub fn parse_smbios(data: &[u8]) -> (Option<String>, Option<String>) {
    let (mut board, mut uuid) = (None, None);
    let mut at = 0usize;
    while at + 4 <= data.len() {
        let kind = data[at];
        let len = data[at + 1] as usize;
        if len < 4 || at + len > data.len() {
            break;
        }
        let formatted = &data[at..at + len];
        // The strings follow the formatted part, each ending in a zero, the set ending in two.
        let mut strings = Vec::new();
        let mut p = at + len;
        let mut start = p;
        while p < data.len() {
            if data[p] == 0 {
                if p == start {
                    break;
                }
                strings.push(String::from_utf8_lossy(&data[start..p]).trim().to_string());
                start = p + 1;
                if p + 1 < data.len() && data[p + 1] == 0 {
                    p += 1;
                    break;
                }
            }
            p += 1;
        }
        let text = |i: usize| -> Option<String> { formatted.get(i).and_then(|&n| (n as usize).checked_sub(1)).and_then(|n| strings.get(n).cloned()) };
        match kind {
            1 if len >= 0x18 => {
                let id = &formatted[8..24];
                if id.iter().any(|&b| b != 0) && id.iter().any(|&b| b != 0xFF) {
                    uuid = Some(id.iter().map(|b| format!("{b:02x}")).collect());
                }
            }
            2 if len >= 8 => {
                let parts: Vec<String> = [text(4), text(5), text(7)].into_iter().flatten().filter(|s| !s.is_empty()).collect();
                // The serial is what tells one board from another; without it the part says too little.
                if text(7).is_some_and(|s| meaningful(&s)) {
                    board = Some(parts.join("|"));
                }
            }
            127 => break,
            _ => {}
        }
        // Past the strings: one zero after the last one, or two when there are none.
        at = if strings.is_empty() { at + len + 2 } else { p + 1 };
    }
    (board, uuid)
}

/// The serial of the physical disk the system lives on.
unsafe fn system_disk_serial() -> Option<String> {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let volume = open(&format!("\\\\.\\{}", drive.trim_end_matches('\\')))?;
    // VOLUME_DISK_EXTENTS: how many extents, then each one's disk number and range.
    let mut extents = [0u8; 64];
    let mut got = 0u32;
    const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;
    let ok = DeviceIoControl(volume, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, std::ptr::null(), 0, extents.as_mut_ptr() as *mut c_void, extents.len() as u32, &mut got, std::ptr::null_mut());
    CloseHandle(volume);
    if ok == 0 || got < 16 {
        return None;
    }
    let disk_number = u32::from_le_bytes([extents[8], extents[9], extents[10], extents[11]]);
    let disk = open(&format!("\\\\.\\PhysicalDrive{disk_number}"))?;
    // STORAGE_PROPERTY_QUERY { StorageDeviceProperty, PropertyStandardQuery }.
    let query = [0u8; 12];
    let mut descriptor = vec![0u8; 1024];
    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
    let ok = DeviceIoControl(disk, IOCTL_STORAGE_QUERY_PROPERTY, query.as_ptr() as *const c_void, query.len() as u32, descriptor.as_mut_ptr() as *mut c_void, descriptor.len() as u32, &mut got, std::ptr::null_mut());
    CloseHandle(disk);
    if ok == 0 || got < 36 {
        return None;
    }
    // STORAGE_DEVICE_DESCRIPTOR: the serial's offset is its seventh dword.
    let offset = u32::from_le_bytes([descriptor[24], descriptor[25], descriptor[26], descriptor[27]]) as usize;
    if offset == 0 || offset >= got as usize {
        return None;
    }
    let end = descriptor[offset..got as usize].iter().position(|&b| b == 0).map_or(got as usize, |n| offset + n);
    let serial = String::from_utf8_lossy(&descriptor[offset..end]).trim().to_string();
    (!serial.is_empty()).then_some(serial)
}

/// A device opened only to be asked about (no reading, no writing: an ordinary user may).
unsafe fn open(path: &str) -> Option<*mut c_void> {
    let handle = CreateFileW(wide(path).as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut());
    (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then_some(handle)
}

unsafe fn system_volume_serial() -> Option<String> {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let root = wide(&format!("{}\\", drive.trim_end_matches('\\')));
    let mut serial = 0u32;
    let ok = GetVolumeInformationW(root.as_ptr(), std::ptr::null_mut(), 0, &mut serial, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
    (ok != 0 && serial != 0).then(|| format!("{serial:08x}"))
}

/// The TPM's endorsement key (its public half), through Windows' own provider for it: an ordinary user may read it, no
/// administrator. RSA as most chips have it, else the ECC one. None: no TPM 2.0, or it is switched off in the BIOS.
unsafe fn tpm() -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{NCryptFreeObject, NCryptGetProperty, NCryptOpenStorageProvider, NCRYPT_PROV_HANDLE};
    let mut provider: NCRYPT_PROV_HANDLE = 0;
    if NCryptOpenStorageProvider(&mut provider, wide("Microsoft Platform Crypto Provider").as_ptr(), 0) != 0 {
        return None;
    }
    let mut found = None;
    for property in ["PCP_EKPUB", "PCP_ECC_EKPUB"] {
        let name = wide(property);
        let mut size = 0u32;
        if NCryptGetProperty(provider, name.as_ptr(), std::ptr::null_mut(), 0, &mut size, 0) != 0 || size == 0 || size > 4096 {
            continue;
        }
        let mut key = vec![0u8; size as usize];
        if NCryptGetProperty(provider, name.as_ptr(), key.as_mut_ptr(), size, &mut size, 0) == 0 {
            key.truncate(size as usize);
            found = Some(key.iter().map(|b| format!("{b:02x}")).collect::<String>());
            break;
        }
    }
    NCryptFreeObject(provider);
    found
}

/// The video card's own id, where its maker gives every card one: NVIDIA's UUID, through the driver's own NVML (an
/// ordinary user may ask; it is the driver's, in System32, so no other nvml.dll is taken). Several cards: the first of
/// their ids in order. AMD's and Intel's ids say where a card sits, not which card it is (every such laptop would be
/// one): those are left out. None: no NVIDIA card, or its driver too old to have NVML there.
unsafe fn gpu() -> Option<String> {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    type Call = unsafe extern "C" fn() -> i32;
    type Count = unsafe extern "C" fn(*mut u32) -> i32;
    type Handle = unsafe extern "C" fn(u32, *mut *mut c_void) -> i32;
    type Uuid = unsafe extern "C" fn(*mut c_void, *mut u8, u32) -> i32;
    let lib = LoadLibraryExW(wide("nvml.dll").as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
    if lib.is_null() {
        return None;
    }
    let ids = (|| {
        let f = |name: &[u8]| GetProcAddress(lib, name.as_ptr());
        let init: Call = std::mem::transmute(f(b"nvmlInit_v2\0")?);
        let shutdown: Call = std::mem::transmute(f(b"nvmlShutdown\0")?);
        let count: Count = std::mem::transmute(f(b"nvmlDeviceGetCount_v2\0")?);
        let handle: Handle = std::mem::transmute(f(b"nvmlDeviceGetHandleByIndex_v2\0")?);
        let uuid: Uuid = std::mem::transmute(f(b"nvmlDeviceGetUUID\0")?);
        if init() != 0 {
            return None;
        }
        let mut ids = Vec::new();
        let mut n = 0u32;
        if count(&mut n) == 0 {
            for i in 0..n.min(8) {
                let mut device = std::ptr::null_mut();
                let mut text = [0u8; 96];
                if handle(i, &mut device) == 0 && uuid(device, text.as_mut_ptr(), text.len() as u32) == 0 {
                    let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
                    ids.push(String::from_utf8_lossy(&text[..end]).trim().to_string());
                }
            }
        }
        shutdown();
        Some(ids)
    })();
    FreeLibrary(lib);
    ids?.into_iter().filter(|id| meaningful(id)).min()
}

/// The monitors on now, each by its maker, model and serial out of its EDID (what a monitor tells the card of itself;
/// Windows keeps it under the monitor's device). The first of them in order, so a second monitor plugged in or out
/// leaves the part as it was unless it is that one. None: no monitor tells its serial (a laptop's own screen seldom
/// does).
unsafe fn monitor() -> Option<String> {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ACTIVE};
    const EDD_GET_DEVICE_INTERFACE_NAME: u32 = 1;
    let text = |w: &[u16]| String::from_utf16_lossy(&w[..w.iter().position(|&c| c == 0).unwrap_or(w.len())]);
    let mut found = Vec::new();
    for a in 0..16 {
        let mut card: DISPLAY_DEVICEW = std::mem::zeroed();
        card.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        if EnumDisplayDevicesW(std::ptr::null(), a, &mut card, 0) == 0 {
            break;
        }
        for m in 0..8 {
            let mut screen: DISPLAY_DEVICEW = std::mem::zeroed();
            screen.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
            if EnumDisplayDevicesW(card.DeviceName.as_ptr(), m, &mut screen, EDD_GET_DEVICE_INTERFACE_NAME) == 0 {
                break;
            }
            if screen.StateFlags & DISPLAY_DEVICE_ACTIVE == 0 {
                continue;
            }
            // \\?\DISPLAY#DELA0F1#5&2a1b3c4d&0&UID4352#{e6f07b5f-…}: the monitor's own key under Enum\DISPLAY
            let id = text(&screen.DeviceID);
            let at: Vec<&str> = id.split('#').collect();
            if at.len() < 4 || !at[0].to_ascii_uppercase().ends_with("DISPLAY") {
                continue;
            }
            let key = format!("SYSTEM\\CurrentControlSet\\Enum\\DISPLAY\\{}\\{}\\Device Parameters", at[1], at[2]);
            let mut edid = [0u8; 512];
            let mut size = edid.len() as u32;
            let status = RegGetValueW(HKEY_LOCAL_MACHINE, wide(&key).as_ptr(), wide("EDID").as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), edid.as_mut_ptr() as *mut c_void, &mut size);
            if status == 0 {
                found.extend(edid_serial(&edid[..size as usize]));
            }
        }
    }
    found.into_iter().min()
}

/// A monitor out of its EDID: maker (three letters), model and serial — the serial as text where it says one (a
/// descriptor of its own, tag 0xFF), else the number in the header. None: no serial, or one every monitor of the
/// kind has (0, 0x01010101).
pub fn edid_serial(e: &[u8]) -> Option<String> {
    if e.len() < 128 || e[..8] != [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0] {
        return None;
    }
    let maker = u16::from_be_bytes([e[8], e[9]]);
    let letters: String = [(maker >> 10) & 31, (maker >> 5) & 31, maker & 31].iter().map(|&c| char::from(b'@' + c as u8)).collect();
    let model = u16::from_le_bytes([e[10], e[11]]);
    let number = u32::from_le_bytes([e[12], e[13], e[14], e[15]]);
    let written = (0..4)
        .map(|i| &e[54 + 18 * i..72 + 18 * i])
        .find(|d| d[..3] == [0, 0, 0] && d[3] == 0xFF)
        .map(|d| String::from_utf8_lossy(&d[5..]).split('\n').next().unwrap_or("").trim().to_string())
        .filter(|s| meaningful(s));
    let serial = written.or_else(|| (number != 0 && number != 0x0101_0101 && meaningful(&format!("{number:08x}"))).then(|| format!("{number:08x}")))?;
    Some(format!("{letters}|{model:04x}|{serial}"))
}

/// Windows' key as the maker put it into the firmware (the ACPI table MSDM of a laptop or a computer sold with
/// Windows): one to a machine. None: a computer put together by hand has none.
unsafe fn msdm() -> Option<String> {
    const ACPI: u32 = u32::from_be_bytes(*b"ACPI");
    let table = u32::from_le_bytes(*b"MSDM");
    let size = GetSystemFirmwareTable(ACPI, table, std::ptr::null_mut(), 0);
    if size == 0 || size > 4096 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    if GetSystemFirmwareTable(ACPI, table, buf.as_mut_ptr(), size) != size {
        return None;
    }
    parse_msdm(&buf)
}

/// The key out of the table: its header (36 bytes), the version, a reserved word, the data's kind, another reserved
/// word, the data's length, then the key itself (29 letters: five groups of five).
pub fn parse_msdm(t: &[u8]) -> Option<String> {
    if t.len() < 56 || &t[..4] != b"MSDM" {
        return None;
    }
    let length = u32::from_le_bytes([t[52], t[53], t[54], t[55]]) as usize;
    let key = t.get(56..56 + length.min(64))?;
    let key = String::from_utf8_lossy(key).trim_matches(char::from(0)).trim().to_string();
    (key.len() >= 25 && meaningful(&key)).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_is_not_a_part() {
        for v in ["To Be Filled By O.E.M.", "Default string", "0000000000", "FFFFFFFF-FFFF", "  ", "ab"] {
            assert!(!meaningful(v), "{v}");
        }
        for v in ["S3Z8NB0KB12345", "PF2ABCDE", "4c4c4544-0039-3910"] {
            assert!(meaningful(v), "{v}");
        }
    }

    #[test]
    fn smbios_gives_the_board_and_the_uuid() {
        let mut t = Vec::new();
        // Type 1, length 0x1B: the UUID at 8.
        let mut sys = vec![1u8, 0x1B, 0, 0, 1, 2, 3, 0];
        sys.extend((1..=16u8).collect::<Vec<_>>());
        sys.extend([0u8; 3]);
        t.extend(&sys);
        t.extend(b"Maker\0Model\0\0");
        // Type 2: maker 1, product 2, version 3, serial 4.
        t.extend([2u8, 8, 1, 0, 1, 2, 3, 4]);
        t.extend(b"ASUS\0PRIME B550\0Rev 1\0MB-SERIAL-42\0\0");
        t.extend([127u8, 4, 2, 0, 0, 0]);
        let (board, uuid) = parse_smbios(&t);
        assert_eq!(board.as_deref(), Some("ASUS|PRIME B550|MB-SERIAL-42"));
        assert_eq!(uuid.as_deref(), Some("0102030405060708090a0b0c0d0e0f10"));
    }

    #[test]
    fn a_board_without_a_serial_is_left_out() {
        let mut t = Vec::new();
        t.extend([2u8, 8, 1, 0, 1, 2, 0, 3]);
        t.extend(b"MSI\0B450\0Default string\0\0");
        assert_eq!(parse_smbios(&t).0, None);
    }

    #[test]
    fn the_hash_is_named_and_stable() {
        assert_eq!(hash("win", "x"), hash("win", "x"));
        assert_ne!(hash("win", "x"), hash("cpu", "x"));
        assert_eq!(hash("win", "x").len(), 64);
    }

    /// An EDID's serial: written out where the monitor says one, else its number; the kind's own numbers are none.
    #[test]
    fn a_monitor_by_its_edid() {
        let mut e = vec![0u8; 128];
        e[..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        // «DEL»: D=4, E=5, L=12, five bits each
        e[8..10].copy_from_slice(&((4u16 << 10) | (5 << 5) | 12).to_be_bytes());
        e[10..12].copy_from_slice(&0xA0F1u16.to_le_bytes());
        e[12..16].copy_from_slice(&0x0101_0101u32.to_le_bytes());
        assert_eq!(edid_serial(&e), None, "the kind's own number, no text");
        e[12..16].copy_from_slice(&0x4C30_3134u32.to_le_bytes());
        assert_eq!(edid_serial(&e).as_deref(), Some("DEL|a0f1|4c303134"));
        // the serial as text in the second descriptor
        e[72..77].copy_from_slice(&[0, 0, 0, 0xFF, 0]);
        e[77..90].copy_from_slice(b"7MT0123ABCD\n ");
        assert_eq!(edid_serial(&e).as_deref(), Some("DEL|a0f1|7MT0123ABCD"));
        assert_eq!(edid_serial(&e[..100]), None, "too short");
        e[0] = 1;
        assert_eq!(edid_serial(&e), None, "not an EDID");
    }

    #[test]
    fn windows_key_out_of_the_firmware() {
        let mut t = vec![0u8; 56];
        t[..4].copy_from_slice(b"MSDM");
        t[52..56].copy_from_slice(&29u32.to_le_bytes());
        t.extend(b"ABCDE-FGHJK-MNPQR-STVWX-YZ234");
        assert_eq!(parse_msdm(&t).as_deref(), Some("ABCDE-FGHJK-MNPQR-STVWX-YZ234"));
        t[..4].copy_from_slice(b"SLIC");
        assert_eq!(parse_msdm(&t), None);
        assert_eq!(parse_msdm(&t[..40]), None);
    }

    /// On a real computer: at least three parts, all valid for the server.
    #[test]
    fn this_computer_has_enough_parts() {
        let started = std::time::Instant::now();
        let p = read();
        eprintln!("parts here ({:?}): {:?}", started.elapsed(), p.keys().collect::<Vec<_>>());
        assert!((3..=12).contains(&p.len()), "{:?}", p.keys().collect::<Vec<_>>());
        // her computer has a TPM 2.0 (15.10): its key is a part
        assert!(p.contains_key("tpm"), "{:?}", p.keys().collect::<Vec<_>>());
        assert!(p.iter().all(|(k, v)| k.bytes().all(|b| b.is_ascii_lowercase()) && v.len() == 64));
        assert_eq!(p, read(), "the same every time");
        assert_eq!(parts(), parts());
    }
}
