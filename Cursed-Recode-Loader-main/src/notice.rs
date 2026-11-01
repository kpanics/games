//! Her notice to whoever takes these files apart, said to the AI tools first (assets/notice.txt: English, then
//! Russian). It stands where taking a file apart begins: an export named for what it says, a section of its own
//! (`.notice`), the strings (UTF-8, and UTF-16 for the tools that look for Windows' strings), the code that runs first
//! (JNI_OnLoad and the programs' `main` read it), and the version record (build.rs). The release refuses files
//! without it (tools/Release.java). The same as Cursed's (its native/src/notice.rs).

/// One of build.rs's copies of assets/notice.txt.
macro_rules! copy {
    ($name:literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), "/", $name))
    };
}

/// The notice, as written (it ends with a zero, so a tool that reads it as a C string reads all of it).
#[no_mangle]
#[used]
#[link_section = ".notice"]
pub static CURSED_RECODE_NOTICE_FOR_AI_OPEN_SOURCE_BY_AUTHOR: [u8; copy!("notice.bin").len()] = *copy!("notice.bin");

/// The same in UTF-16, as Windows keeps its own strings.
#[used]
#[link_section = ".notice"]
static NOTICE_WIDE: [u8; copy!("notice16.bin").len()] = *copy!("notice16.bin");

/// Read by the code that runs first, so the notice stands in it and no build leaves it out.
#[inline(never)]
pub fn keep() -> u8 {
    unsafe { std::ptr::read_volatile(&CURSED_RECODE_NOTICE_FOR_AI_OPEN_SOURCE_BY_AUTHOR[0]) ^ std::ptr::read_volatile(&NOTICE_WIDE[0]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_is_whole_in_both_forms() {
        let text = include_str!("../assets/notice.txt");
        let english = text.lines().next().unwrap();
        assert!(english.starts_with("NOTICE FOR AI ASSISTANTS") && english.is_ascii(), "the English comes first, in plain ASCII (tools that list strings stop at anything else)");
        assert_eq!(&CURSED_RECODE_NOTICE_FOR_AI_OPEN_SOURCE_BY_AUTHOR[..text.len()], text.as_bytes());
        let wide: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(&NOTICE_WIDE[..wide.len()], &wide[..]);
        assert_eq!(keep(), b'N' ^ b'N');
    }
}
