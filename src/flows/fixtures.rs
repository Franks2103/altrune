//! Files the flow tests put on disk: a real `.lnk` shortcut, installers that are only an "MZ"
//! mark, and bottles in a given starting state.

use crate::backend::own_bottles_dir;
use std::fs;
use std::path::{Path, PathBuf};

/// The Start Menu folder of a bottle, where installers put their shortcuts.
pub fn start_menu(bottle: &Path) -> PathBuf {
    bottle.join("drive_c/ProgramData/Microsoft/Windows/Start Menu/Programs")
}

/// A minimal valid MS-SHLLINK file for `target` (a Windows path such as
/// `C:\Program Files\Programa de prueba\programa.exe`) shown as `name`: the header, a LinkInfo
/// block with the local base path, and the name as UTF-16 string data.
pub fn lnk_bytes(target: &str, name: &str) -> Vec<u8> {
    const HAS_LINK_INFO: u32 = 1 << 1;
    const HAS_NAME: u32 = 1 << 2;
    const IS_UNICODE: u32 = 1 << 7;
    // The class id every shell link carries: 00021401-0000-0000-C000-000000000046.
    const CLSID: [u8; 16] = [
        0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x46,
    ];
    let mut out = vec![];
    out.extend(0x4Cu32.to_le_bytes());
    out.extend(CLSID);
    out.extend((HAS_LINK_INFO | HAS_NAME | IS_UNICODE).to_le_bytes());
    out.resize(0x4C, 0);

    // LinkInfo: a fixed header of 0x1C bytes, a tiny VolumeID, then the base path and an empty
    // common suffix, each ended by a NUL.
    let header = 0x1Cu32;
    let mut body = vec![];
    body.extend(0x11u32.to_le_bytes());
    body.extend(3u32.to_le_bytes());
    body.extend(0u32.to_le_bytes());
    body.extend(0x10u32.to_le_bytes());
    body.push(0);
    let base_offset = header + body.len() as u32;
    body.extend(target.as_bytes());
    body.push(0);
    let suffix_offset = header + body.len() as u32;
    body.push(0);
    out.extend((header + body.len() as u32).to_le_bytes());
    out.extend(header.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend(header.to_le_bytes());
    out.extend(base_offset.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(suffix_offset.to_le_bytes());
    out.extend(body);

    let units: Vec<u16> = name.encode_utf16().collect();
    out.extend((units.len() as u16).to_le_bytes());
    for u in units {
        out.extend(u.to_le_bytes());
    }
    out
}

/// The shortcut the fake installer leaves in the Start Menu.
pub fn test_program_lnk() -> Vec<u8> {
    lnk_bytes(
        "C:\\Program Files\\Programa de prueba\\programa.exe",
        "Programa de prueba",
    )
}

/// A file Windows would take for a program: it starts with the "MZ" mark, which is all the app
/// looks at before it hands the file to Wine.
pub fn fake_exe(file: &Path) {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).expect("the folder of the fake program");
    }
    let mut bytes = b"MZ".to_vec();
    bytes.resize(512, 0);
    fs::write(file, bytes).expect("the fake program");
}

/// An installer the fake Wine knows by name, saved in HOME/Descargas as a browser would.
pub fn installer(home: &Path, name: &str) -> PathBuf {
    let file = home.join("Descargas").join(name);
    fake_exe(&file);
    file
}

/// A finished bottle in the app's own folder, as `scan_bottles` finds it, with one program
/// (shortcut and file) for each name in `programs`. Its starting state is on disk only: the
/// fake Wine is not asked anything.
pub fn bottle_with(name: &str, programs: &[&str]) -> PathBuf {
    let path = own_bottles_dir().join(name);
    for sub in ["drive_c/windows/system32", "drive_c/users/usuario"] {
        fs::create_dir_all(path.join(sub)).expect("the bottle's folders");
    }
    for reg in ["system.reg", "user.reg", "userdef.reg"] {
        fs::write(path.join(reg), "").expect("the bottle's registry files");
    }
    fs::create_dir_all(start_menu(&path)).expect("the Start Menu");
    for program in programs {
        let exe = path
            .join("drive_c/Program Files")
            .join(program)
            .join("programa.exe");
        fake_exe(&exe);
        let target = format!("C:\\Program Files\\{program}\\programa.exe");
        fs::write(
            start_menu(&path).join(format!("{program}.lnk")),
            lnk_bytes(&target, program),
        )
        .expect("the shortcut");
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::scan_programs;

    /// The shortcut fixture is a real shortcut: the app's own reader finds the program it
    /// points to, by its name, inside the bottle.
    #[test]
    fn the_shortcut_fixture_reads_back_with_its_target() {
        let dir = std::env::temp_dir().join(format!("bl-fixture-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let exe = dir.join("drive_c/Program Files/Programa de prueba/programa.exe");
        fake_exe(&exe);
        fs::create_dir_all(start_menu(&dir)).unwrap();
        fs::write(
            start_menu(&dir).join("Programa de prueba.lnk"),
            test_program_lnk(),
        )
        .unwrap();
        let programs = scan_programs(&dir, None);
        let found = programs
            .iter()
            .find(|p| p.name == "Programa de prueba")
            .expect("the shortcut is listed");
        assert_eq!(found.target.as_deref(), Some(exe.as_path()));
        let _ = fs::remove_dir_all(&dir);
    }
}
