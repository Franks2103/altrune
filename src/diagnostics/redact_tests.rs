//! Tests of redaction, with realistic Wine lines and a made-up user.

use super::tests::lucia;
use super::*;

// ---- redaction: realistic Wine lines, with a made-up user ----

#[test]
fn redacts_the_home_folder_and_the_user_in_every_path_form() {
    let r = lucia();
    for (line, hidden) in [
        ("open /Users/lucia/Library/x", "~/Library/x"),
        (
            "Z:\\Users\\lucia\\Downloads\\setup.exe",
            "\\Users\\<usuario>\\Downloads",
        ),
        (
            "C:\\users\\lucia\\AppData\\Local",
            "\\users\\<usuario>\\AppData",
        ),
        (
            "L\"C:\\\\users\\\\Lucia\\\\Documents\\\\g.exe\"",
            "\\\\users\\\\<usuario>\\\\Documents",
        ),
        (
            "err Z:\\\\Users\\\\LUCIA\\\\x",
            "\\\\Users\\\\<usuario>\\\\x",
        ),
        ("/users/LUCIA", "/users/<usuario>"),
        ("path=C:\\users\\lucia", "\\users\\<usuario>"),
    ] {
        let out = r.apply(line);
        assert!(!out.to_lowercase().contains("lucia"), "{line} -> {out}");
        assert!(out.contains(hidden), "{line} -> {out}");
    }
}

#[test]
fn user_names_are_only_hidden_as_whole_folder_names() {
    let r = lucia();
    let out = r.apply("C:\\users\\luciana\\x and C:\\users\\public\\y and users lucia");
    assert!(out.contains("luciana"), "{out}");
    assert!(out.contains("\\users\\public\\"), "{out}");
    assert!(
        out.contains("users lucia"),
        "no path separator, nothing to hide: {out}"
    );
    // A short name is hidden too, but only inside a users folder.
    let r = Redactor::new("/Users/jo", &["jo".to_string()]);
    let out = r.apply("C:\\users\\jo\\x and John and jo.dll");
    assert!(
        out.contains("<usuario>") && out.contains("John") && out.contains("jo.dll"),
        "{out}"
    );
}

#[test]
fn extra_wine_user_names_are_hidden() {
    let r = Redactor::new("/Users/lucia", &["lucia".into(), "Pablo Perez".into()]);
    let out = r.apply("C:\\users\\pablo perez\\Documents and \\users\\lucia");
    assert!(
        !out.to_lowercase().contains("pablo") && !out.contains("lucia"),
        "{out}"
    );
}

#[test]
fn redacts_words_split_on_quotes_commas_colons_semicolons_tabs_and_equals() {
    let r = lucia();
    let out = r.apply("{\"email\":\"lucia@example.com\",\"steam\":\"76561198012345678\"}");
    assert!(
        out.contains("<correo>") && out.contains("<id-steam>"),
        "{out}"
    );
    assert!(
        !out.contains("example.com") && !out.contains("7656119"),
        "{out}"
    );
    let out = r.apply("user\tlucia@example.com\tok");
    assert_eq!(out, "user\t<correo>\tok");
    let out = r.apply("a;b@c.org;d");
    assert_eq!(out, "a;<correo>;d");
    let out = r.apply("session:aGVsbG8gd29ybGQgdGhpcyBpcyBhIGxvbmcgdG9rZW4x,next");
    assert_eq!(out, "session:<oculto>,next");
    assert_eq!(r.apply("plain words stay"), "plain words stay");
}

#[test]
fn redacts_steam_ids_and_launcher_arguments() {
    let r = lucia();
    let out = r.apply("steam.exe -steamid=76561198012345678 -rline_launch=1 -BranchAccess=beta");
    assert!(out.contains("-steamid=<oculto>"), "{out}");
    assert!(!out.contains("7656119"), "{out}");
    assert!(
        !out.to_lowercase().contains("rline_") && !out.to_lowercase().contains("branchaccess"),
        "{out}"
    );
    let out =
        r.apply("friend id 76561198012345678 and number 12345678901234567 and 765611980123456789");
    assert!(out.contains("<id-steam>"), "{out}");
    assert!(
        out.contains("12345678901234567"),
        "other numbers stay: {out}"
    );
    assert!(
        out.contains("765611980123456789"),
        "18 digits is not a Steam id: {out}"
    );
}

#[test]
fn redacts_tokens_and_json_web_tokens() {
    let r = lucia();
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r";
    let out = r.apply(&format!("Authorization: Bearer {jwt} done"));
    assert!(!out.contains("eyJ") && out.contains("done"), "{out}");
    let out = r.apply(&format!("{{\"token\":\"{jwt}\"}}"));
    assert!(!out.contains("eyJ"), "{out}");
    let out = r.apply("GET /login?token=abc123&lang=es failed");
    assert_eq!(out, "GET /login?token=<oculto>&lang=es failed");
    let out = r.apply("access_token=zzz auth=qq password=hunter2 x");
    assert_eq!(
        out,
        "access_token=<oculto> auth=<oculto> password=<oculto> x"
    );
    let out = r.apply("bad password for the account");
    assert!(!out.contains("password"), "{out}");
    // A dotted name that is not a token stays readable.
    assert_eq!(
        r.apply("Microsoft.VC90.CRT.dll ok"),
        "Microsoft.VC90.CRT.dll ok"
    );
}

/// Names and words with accents must never break the scan (slicing inside a letter panics).
/// Quoted values, JSON pairs and longer key names must stay hidden: the exported report is
/// made to be shared.
#[test]
fn redacts_quoted_json_and_longer_secret_names() {
    let r = lucia();
    for (line, secret) in [
        ("cmd -token=\"abc123\" -user foo", "abc123"),
        ("launch --password=\"hunter2 x\" ok", "hunter2"),
        ("{\"password\":\"hunter2\",\"user\":\"bob\"}", "hunter2"),
        ("{\"access_token\":\"abc123def\",\"a\":1}", "abc123def"),
        ("{\"token\": \"abc123def\"}", "abc123def"),
        ("{\\\"token\\\":\\\"abc123def\\\"}", "abc123def"),
        ("GET /x?oauth=abcdef123 HTTP", "abcdef123"),
        ("X-Auth=secretvalue", "secretvalue"),
        ("passwordhash=abc123", "abc123"),
        ("launch -user_password_hash=zzz9", "zzz9"),
        ("password: hunter2", "hunter2"),
        ("Authorization: Bearer abc123def", "abc123def"),
        ("Authorization: Basic dXNlcjpwYXNz", "dXNlcjpwYXNz"),
        ("client_secret='s3cr3t' next", "s3cr3t"),
    ] {
        let out = r.apply(line);
        assert!(!out.contains(secret), "{line} -> {out}");
    }
    // What is around the secret stays readable.
    assert_eq!(
        r.apply("cmd -token=\"abc123\" -user foo"),
        "cmd -token=\"<oculto>\" -user foo"
    );
    assert!(r
        .apply("{\"access_token\":\"abc123def\",\"a\":1}")
        .ends_with(",\"a\":1}"));
    assert_eq!(
        r.apply("path C:\\Games\\x.exe ok"),
        "path C:\\Games\\x.exe ok"
    );
    assert_eq!(
        r.apply("err:module: Library x.dll not found"),
        "err:module: Library x.dll not found"
    );
}

#[test]
fn redaction_handles_non_ascii_text() {
    let r = Redactor::new("/Users/ñandú", &["ñandú".to_string()]);
    let out = r.apply("é=ü ñ=token=x C:\\users\\Ñandú\\Escritorio ✗ «a@b.co» users\\ñ");
    assert!(!out.to_lowercase().contains("ñandú"), "{out}");
    assert!(
        out.contains("Escritorio") && out.contains("<correo>"),
        "{out}"
    );
    assert_eq!(r.apply("contraseña=ñ"), "contraseña=ñ");
    assert_eq!(r.apply("é-token=ñ ok"), "é-token=<oculto> ok");
}

#[test]
fn redaction_keeps_what_a_diagnosis_needs() {
    let r = lucia();
    let line = "0024:err:module:import_dll Library MSVCP140.dll (which is needed by L\"C:\\\\Users\\\\lucia\\\\Games\\\\g.exe\") not found";
    let out = r.apply(line);
    assert!(
        out.contains("MSVCP140.dll") && out.contains("not found"),
        "{out}"
    );
    assert!(!out.contains("lucia"), "{out}");
}

#[test]
fn redacts_short_steam_ids_and_login_cookies() {
    let r = lucia();
    for line in [
        "open C:\\Program Files (x86)\\Steam\\userdata\\123456789\\config\\localconfig.vdf",
        "open /steam/userdata/123456789/config",
        "user [U:1:123456789] joined",
        "owner STEAM_0:1:61728394 ok",
    ] {
        let out = r.apply(line);
        assert!(out.contains("<id-steam>"), "{out}");
        assert!(
            !out.contains("123456789") && !out.contains("61728394"),
            "{out}"
        );
    }
    assert!(r.apply("userdata/config").contains("userdata/config"));
    assert_eq!(r.apply("steam_api 12345"), "steam_api 12345");
    let out = r.apply("steamLoginSecure=76561198012345678%7C%7CeyAiJ0eXAi.abc.def; x=1");
    assert!(!out.contains("eyAi") && !out.contains("7656119"), "{out}");
    let out = r.apply("Cookie: steamLoginSecure=abc%7C%7Cxyz; sessionid=q1w2");
    assert!(!out.contains("abc%7C") && !out.contains("q1w2"), "{out}");
}

/// The name of the Mac's user comes in from outside (this module looks up no account): the one
/// that is given is hidden, with the extra names, and with none given only the extras are.
#[test]
fn the_mac_user_is_the_one_that_is_given() {
    let line = "Z:\\Users\\lucia\\x and /Users/Pablo/y and C:\\users\\legacy\\z";
    let both = Redactor::for_this_mac("lucia", &["Pablo".to_string()]).apply(line);
    assert!(!both.contains("lucia") && !both.contains("Pablo"), "{both}");
    assert!(both.contains("legacy"), "{both}");
    let only_extra = Redactor::for_this_mac("", &["Pablo".to_string()]).apply(line);
    assert!(only_extra.contains("lucia"), "{only_extra}");
    assert!(!only_extra.contains("Pablo"), "{only_extra}");
    let alone = redact("lucia", "see C:\\users\\LUCIA\\AppData");
    assert_eq!(alone, "see C:\\users\\<usuario>\\AppData");
}
