//! A small, defensive reader of Windows shortcuts (`.lnk`, the Shell Link binary format, MS-SHLLINK).
//!
//! Only what the program library needs is read: the target the shortcut runs, its arguments,
//! its working folder and its icon location. Every offset and length comes from the file, so
//! every read is bounds-checked; a file that is malformed or cut short gives `None`, never a panic.

/// What a shortcut says. Paths are Windows paths exactly as written in the file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellLink {
    /// The file or folder the shortcut runs, when the shortcut says it as a local path.
    pub target: Option<String>,
    /// The text shown for the shortcut (the description), if any.
    pub name: String,
    /// The target relative to the shortcut's own folder, if given.
    pub relative_path: String,
    pub working_dir: String,
    pub arguments: String,
    /// Where the shortcut's picture is: `C:\path\app.ico` or `C:\path\app.exe`, without the index.
    pub icon_location: Option<String>,
}

const HEADER_SIZE: u32 = 0x4C;
/// The class id every shell link carries: 00021401-0000-0000-C000-000000000046.
const LINK_CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

const HAS_LINK_TARGET_ID_LIST: u32 = 1 << 0;
const HAS_LINK_INFO: u32 = 1 << 1;
const HAS_NAME: u32 = 1 << 2;
const HAS_RELATIVE_PATH: u32 = 1 << 3;
const HAS_WORKING_DIR: u32 = 1 << 4;
const HAS_ARGUMENTS: u32 = 1 << 5;
const HAS_ICON_LOCATION: u32 = 1 << 6;
const IS_UNICODE: u32 = 1 << 7;

const VOLUME_ID_AND_LOCAL_BASE_PATH: u32 = 1;

/// Reads bytes at positions the file chose, refusing anything outside the file.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes(2)?.try_into().ok()?))
    }
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    Some(u32::from_le_bytes(data.get(at..end)?.try_into().ok()?))
}

/// Text in the system's ANSI code page: UTF-8 when it is valid, else one byte per character.
fn ansi(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// A zero-terminated ANSI string starting at `at` inside `data`.
fn c_string(data: &[u8], at: usize) -> Option<String> {
    let rest = data.get(at..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    Some(ansi(&rest[..end]))
}

/// A zero-terminated UTF-16 string starting at `at` inside `data`.
fn wide_string(data: &[u8], at: usize) -> Option<String> {
    let rest = data.get(at..)?;
    let mut units = vec![];
    for pair in rest.as_chunks::<2>().0 {
        let u = u16::from_le_bytes(*pair);
        if u == 0 {
            return Some(String::from_utf16_lossy(&units));
        }
        units.push(u);
    }
    None
}

/// The local path of the LinkInfo block: base path plus the common suffix.
fn link_info_target(info: &[u8]) -> Option<String> {
    let header_size = u32_at(info, 4)? as usize;
    let flags = u32_at(info, 8)?;
    if flags & VOLUME_ID_AND_LOCAL_BASE_PATH == 0 {
        return None;
    }
    let base_offset = u32_at(info, 16)? as usize;
    let suffix_offset = u32_at(info, 24)? as usize;
    // From header size 0x24 on, the same two strings are also given in UTF-16.
    let unicode = if header_size >= 0x24 {
        let base = u32_at(info, 28)? as usize;
        let suffix = u32_at(info, 32)? as usize;
        Some((base, suffix))
    } else {
        None
    };
    let (base, suffix) = match unicode {
        Some((b, s)) if b != 0 => (
            wide_string(info, b)?,
            if s != 0 {
                wide_string(info, s).unwrap_or_default()
            } else {
                String::new()
            },
        ),
        _ => (
            c_string(info, base_offset)?,
            if suffix_offset != 0 {
                c_string(info, suffix_offset).unwrap_or_default()
            } else {
                String::new()
            },
        ),
    };
    let full = format!("{base}{suffix}");
    (!full.trim().is_empty()).then_some(full)
}

/// One StringData entry: a 16-bit character count, then that many characters. Wine counts the
/// terminating NUL as one of them (an empty argument list is stored as "\0"), so trailing NULs
/// are dropped: otherwise two shortcuts to the same program would not look alike.
fn string_data(r: &mut Reader, unicode: bool) -> Option<String> {
    let count = r.u16()? as usize;
    let text = if unicode {
        let raw = r.bytes(count.checked_mul(2)?)?;
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        ansi(r.bytes(count)?)
    };
    Some(text.trim_end_matches('\0').to_string())
}

/// Reads a shortcut. `None` when the bytes are not a shell link or end too early.
pub fn parse(data: &[u8]) -> Option<ShellLink> {
    if u32_at(data, 0)? != HEADER_SIZE || data.get(4..20)? != LINK_CLSID {
        return None;
    }
    let flags = u32_at(data, 20)?;
    let mut r = Reader {
        data,
        pos: HEADER_SIZE as usize,
    };
    if flags & HAS_LINK_TARGET_ID_LIST != 0 {
        let size = r.u16()? as usize;
        r.bytes(size)?;
    }
    let mut link = ShellLink::default();
    if flags & HAS_LINK_INFO != 0 {
        let size = u32_at(data, r.pos)? as usize;
        // The block must hold at least its own fixed header.
        if size < 0x1C {
            return None;
        }
        let info = r.bytes(size)?;
        link.target = link_info_target(info);
    }
    let unicode = flags & IS_UNICODE != 0;
    if flags & HAS_NAME != 0 {
        link.name = string_data(&mut r, unicode)?;
    }
    if flags & HAS_RELATIVE_PATH != 0 {
        link.relative_path = string_data(&mut r, unicode)?;
    }
    if flags & HAS_WORKING_DIR != 0 {
        link.working_dir = string_data(&mut r, unicode)?;
    }
    if flags & HAS_ARGUMENTS != 0 {
        link.arguments = string_data(&mut r, unicode)?;
    }
    if flags & HAS_ICON_LOCATION != 0 {
        let loc = string_data(&mut r, unicode)?;
        // "C:\x\app.exe,3": the part after the last comma is the picture's number.
        let path = match loc.rsplit_once(',') {
            Some((p, n))
                if n.trim()
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit()) =>
            {
                p
            }
            _ => &loc,
        };
        let path = path.trim();
        link.icon_location = (!path.is_empty()).then(|| path.to_string());
    }
    Some(link)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A shortcut to `target` with `args`, as bytes, for the tests of the library scan.
    pub(in crate::backend) fn sample_link(target: &str, args: &str) -> Vec<u8> {
        build(&Build {
            target: Some(target),
            args,
            ..Build::default()
        })
    }

    /// Builds a shortcut the way Windows lays it out. `unicode` chooses the text encoding of
    /// StringData; the LinkInfo block carries both the ANSI and the UTF-16 target when `wide_info`.
    struct Build<'a> {
        target: Option<&'a str>,
        suffix: &'a str,
        name: &'a str,
        relative: &'a str,
        dir: &'a str,
        args: &'a str,
        icon: &'a str,
        unicode: bool,
        wide_info: bool,
        id_list: bool,
    }

    impl<'a> Default for Build<'a> {
        fn default() -> Self {
            Build {
                target: None,
                suffix: "",
                name: "",
                relative: "",
                dir: "",
                args: "",
                icon: "",
                unicode: true,
                wide_info: false,
                id_list: false,
            }
        }
    }

    fn put_string(out: &mut Vec<u8>, s: &str, unicode: bool) {
        if unicode {
            let units: Vec<u16> = s.encode_utf16().collect();
            out.extend((units.len() as u16).to_le_bytes());
            for u in units {
                out.extend(u.to_le_bytes());
            }
        } else {
            out.extend((s.len() as u16).to_le_bytes());
            out.extend(s.as_bytes());
        }
    }

    fn link_info(base: &str, suffix: &str, wide: bool) -> Vec<u8> {
        // Header: size, header size, flags, volume id offset, local base path offset,
        // network offset, suffix offset (+ the two UTF-16 offsets for the long header).
        let header = if wide { 0x24 } else { 0x1C };
        let mut body = vec![];
        let volume_id_offset = header as u32;
        // A minimal VolumeID: size 0x10, drive type, serial, label offset 0x10, then one NUL.
        let mut volume = vec![];
        volume.extend(0x11u32.to_le_bytes());
        volume.extend(3u32.to_le_bytes());
        volume.extend(0u32.to_le_bytes());
        volume.extend(0x10u32.to_le_bytes());
        volume.push(0);
        body.extend(&volume);
        let base_offset = (header + body.len()) as u32;
        body.extend(base.as_bytes());
        body.push(0);
        let suffix_offset = (header + body.len()) as u32;
        body.extend(suffix.as_bytes());
        body.push(0);
        let (mut base_w, mut suffix_w) = (0u32, 0u32);
        if wide {
            base_w = (header + body.len()) as u32;
            for u in base.encode_utf16() {
                body.extend(u.to_le_bytes());
            }
            body.extend([0, 0]);
            suffix_w = (header + body.len()) as u32;
            for u in suffix.encode_utf16() {
                body.extend(u.to_le_bytes());
            }
            body.extend([0, 0]);
        }
        let mut info = vec![];
        info.extend(((header + body.len()) as u32).to_le_bytes());
        info.extend((header as u32).to_le_bytes());
        info.extend(1u32.to_le_bytes());
        info.extend(volume_id_offset.to_le_bytes());
        info.extend(base_offset.to_le_bytes());
        info.extend(0u32.to_le_bytes());
        info.extend(suffix_offset.to_le_bytes());
        if wide {
            info.extend(base_w.to_le_bytes());
            info.extend(suffix_w.to_le_bytes());
        }
        info.extend(body);
        info
    }

    fn build(b: &Build) -> Vec<u8> {
        let mut flags = 0u32;
        if b.id_list {
            flags |= HAS_LINK_TARGET_ID_LIST;
        }
        if b.target.is_some() {
            flags |= HAS_LINK_INFO;
        }
        for (set, bit) in [
            (!b.name.is_empty(), HAS_NAME),
            (!b.relative.is_empty(), HAS_RELATIVE_PATH),
            (!b.dir.is_empty(), HAS_WORKING_DIR),
            (!b.args.is_empty(), HAS_ARGUMENTS),
            (!b.icon.is_empty(), HAS_ICON_LOCATION),
        ] {
            if set {
                flags |= bit;
            }
        }
        if b.unicode {
            flags |= IS_UNICODE;
        }
        let mut out = vec![];
        out.extend(HEADER_SIZE.to_le_bytes());
        out.extend(LINK_CLSID);
        out.extend(flags.to_le_bytes());
        out.resize(HEADER_SIZE as usize, 0);
        if b.id_list {
            // Two items of three bytes, and the terminating zero item.
            let list = [5u8, 0, 1, 2, 3, 5, 0, 4, 5, 6, 0, 0];
            out.extend((list.len() as u16).to_le_bytes());
            out.extend(list);
        }
        if let Some(t) = b.target {
            out.extend(link_info(t, b.suffix, b.wide_info));
        }
        for (s, present) in [
            (b.name, !b.name.is_empty()),
            (b.relative, !b.relative.is_empty()),
            (b.dir, !b.dir.is_empty()),
            (b.args, !b.args.is_empty()),
            (b.icon, !b.icon.is_empty()),
        ] {
            if present {
                put_string(&mut out, s, b.unicode);
            }
        }
        out
    }

    #[test]
    fn reads_target_arguments_and_icon_from_a_typical_shortcut() {
        let bytes = build(&Build {
            target: Some("C:\\Program Files\\Foo\\foo.exe"),
            name: "Foo, el programa",
            relative: "..\\..\\Program Files\\Foo\\foo.exe",
            dir: "C:\\Program Files\\Foo",
            args: "-windowed -novid",
            icon: "C:\\Program Files\\Foo\\foo.ico,0",
            id_list: true,
            ..Build::default()
        });
        let link = parse(&bytes).expect("a valid shortcut");
        assert_eq!(
            link.target.as_deref(),
            Some("C:\\Program Files\\Foo\\foo.exe")
        );
        assert_eq!(link.name, "Foo, el programa");
        assert_eq!(link.working_dir, "C:\\Program Files\\Foo");
        assert_eq!(link.arguments, "-windowed -novid");
        assert_eq!(
            link.icon_location.as_deref(),
            Some("C:\\Program Files\\Foo\\foo.ico")
        );
    }

    #[test]
    fn wine_style_counted_nul_reads_as_empty_text() {
        // Wine writes an empty argument list as one NUL character, counted.
        for unicode in [true, false] {
            let bytes = build(&Build {
                target: Some("C:\\Program Files (x86)\\Steam\\steam.exe"),
                dir: "C:\\Program Files (x86)\\Steam\0",
                args: "\0",
                unicode,
                ..Build::default()
            });
            let link = parse(&bytes).unwrap();
            assert_eq!(link.arguments, "", "unicode {unicode}");
            assert_eq!(link.working_dir, "C:\\Program Files (x86)\\Steam");
        }
    }

    #[test]
    fn joins_base_path_and_common_suffix_and_prefers_the_wide_copy() {
        let bytes = build(&Build {
            target: Some("C:\\Juegos\\Señor"),
            suffix: "\\señor.exe",
            wide_info: true,
            ..Build::default()
        });
        let link = parse(&bytes).unwrap();
        assert_eq!(link.target.as_deref(), Some("C:\\Juegos\\Señor\\señor.exe"));
    }

    #[test]
    fn reads_ansi_string_data_and_icon_without_number() {
        let bytes = build(&Build {
            target: Some("C:\\a\\b.exe"),
            args: "/x",
            icon: "C:\\a\\b.ico",
            unicode: false,
            ..Build::default()
        });
        let link = parse(&bytes).unwrap();
        assert_eq!(link.arguments, "/x");
        assert_eq!(link.icon_location.as_deref(), Some("C:\\a\\b.ico"));
    }

    #[test]
    fn a_shortcut_without_a_local_path_has_no_target() {
        let bytes = build(&Build {
            relative: ".\\foo.exe",
            id_list: true,
            ..Build::default()
        });
        let link = parse(&bytes).unwrap();
        assert_eq!(link.target, None);
        assert_eq!(link.relative_path, ".\\foo.exe");
    }

    #[test]
    fn refuses_files_that_are_not_shortcuts() {
        assert_eq!(parse(&[]), None);
        assert_eq!(parse(b"MZ not a shortcut at all"), None);
        let mut wrong_clsid = build(&Build::default());
        wrong_clsid[10] ^= 0xFF;
        assert_eq!(parse(&wrong_clsid), None);
        let mut wrong_size = build(&Build::default());
        wrong_size[0] = 0x4D;
        assert_eq!(parse(&wrong_size), None);
    }

    #[test]
    fn truncated_shortcuts_never_panic() {
        let bytes = build(&Build {
            target: Some("C:\\Program Files\\Foo\\foo.exe"),
            name: "Foo",
            dir: "C:\\Program Files\\Foo",
            args: "-a",
            icon: "C:\\Foo\\foo.ico,1",
            id_list: true,
            wide_info: true,
            ..Build::default()
        });
        assert!(parse(&bytes).is_some());
        // Every shorter prefix is either read as far as it goes or refused, never a panic.
        for n in 0..bytes.len() {
            let _ = parse(&bytes[..n]);
        }
        // Cutting inside the header or the string data is refused.
        assert_eq!(parse(&bytes[..40]), None);
        assert_eq!(parse(&bytes[..bytes.len() - 3]), None);
    }

    #[test]
    fn lying_lengths_and_offsets_are_refused() {
        let good = build(&Build {
            target: Some("C:\\a.exe"),
            args: "-b",
            id_list: true,
            ..Build::default()
        });
        // An ID list claiming more bytes than the file has.
        let mut long_list = good.clone();
        long_list[0x4C] = 0xFF;
        long_list[0x4D] = 0xFF;
        assert_eq!(parse(&long_list), None);
        // A LinkInfo block claiming a huge size.
        let info_at = 0x4C + 2 + 12;
        let mut long_info = good.clone();
        long_info[info_at..info_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&long_info), None);
        // A LinkInfo block too small for its own header.
        let mut tiny_info = good.clone();
        tiny_info[info_at..info_at + 4].copy_from_slice(&4u32.to_le_bytes());
        assert_eq!(parse(&tiny_info), None);
        // A base path offset pointing outside the block: no target, but still a shortcut.
        let mut bad_offset = good.clone();
        bad_offset[info_at + 16..info_at + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&bad_offset).map(|l| l.target), Some(None));
        // A string whose character count runs past the end.
        let mut long_string = good.clone();
        let n = long_string.len();
        long_string[n - 6..n - 4].copy_from_slice(&0xFFFFu16.to_le_bytes());
        assert_eq!(parse(&long_string), None);
    }
}
