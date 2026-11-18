//! Redaction: hides personal data (home folder, user names, e-mail addresses, Steam account
//! numbers, tokens and secrets) in a line before it is shown or put in a report.

/// Removes personal data from a line: the home folder, the user's name inside Mac and Windows
/// paths (any case, single or doubled backslashes), e-mail addresses, Steam account numbers,
/// tokens and secrets. It hides what it knows to look for; the report asks the reader to check
/// it before sharing it.
#[derive(Clone, Debug, Default)]
pub struct Redactor {
    home: String,
    /// Lower-case names that must not appear after `users/` or `users\`, longest first.
    names: Vec<String>,
}

impl Redactor {
    pub fn new(home: &str, names: &[String]) -> Self {
        let mut names: Vec<String> = names
            .iter()
            .map(|n| n.trim().to_lowercase())
            .filter(|n| !n.is_empty())
            .collect();
        names.sort();
        names.dedup();
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        Self {
            home: if home == "/" {
                String::new()
            } else {
                home.to_string()
            },
            names,
        }
    }

    /// For this Mac: its home folder and `mac_user`, the name of the Mac's user, plus `extra`
    /// names (for example the Wine user folders of the bottle being read). The user's name is
    /// given, not looked up: this module reads no account of the Mac itself.
    pub fn for_this_mac(mac_user: &str, extra: &[String]) -> Self {
        let mut names = vec![mac_user.to_string()];
        names.extend(extra.iter().cloned());
        Self::new(&std::env::var("HOME").unwrap_or_default(), &names)
    }

    pub fn apply(&self, line: &str) -> String {
        let mut s = line.to_string();
        if !self.home.is_empty() {
            s = s.replace(&self.home, "~");
        }
        let s = hide_user_folders(&s, &self.names);
        let s = hide_secret_values(&s);
        let s = hide_steam_ids(&s);
        let s = hide_short_steam_ids(&s);
        hide_words(&s)
    }
}

/// Removes personal data with this Mac's own names (see [`Redactor`]).
pub fn redact(mac_user: &str, line: &str) -> String {
    Redactor::for_this_mac(mac_user, &[]).apply(line)
}

/// Whether `rest` (what follows a user name) ends the name: a path separator, a quote, a space…
fn ends_name(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        None => true,
        Some('.') => !chars.next().is_some_and(|c| c.is_alphanumeric()),
        Some(c) => !(c.is_alphanumeric() || c == '_' || c == '-'),
    }
}

/// How many bytes of `text` spell `name` (already lower-case), ignoring case, if it starts
/// with it.
fn name_len(text: &str, name: &str) -> Option<usize> {
    let mut used = 0;
    let mut chars = text.chars();
    for want in name.chars() {
        let c = chars.next()?;
        let mut lower = c.to_lowercase();
        if lower.next() != Some(want) || lower.next().is_some() {
            return None;
        }
        used += c.len_utf8();
    }
    Some(used)
}

/// `/Users/<name>/…`, `C:\users\<name>\…` and Wine's doubled form `C:\\users\\<name>\\…` lose
/// the name, whatever its case, as long as the name is a whole folder name.
fn hide_user_folders(s: &str, names: &[String]) -> String {
    if names.is_empty() {
        return s.to_string();
    }
    // ASCII lower-casing keeps every byte position, so indexes into `low` fit `s`.
    let low = s.to_ascii_lowercase();
    let bytes = low.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut copied = 0;
    let mut from = 0;
    while let Some(off) = low[from..].find("users") {
        let at = from + off;
        from = at + 5;
        if at == 0 || !matches!(bytes[at - 1], b'/' | b'\\') {
            continue;
        }
        let mut j = at + 5;
        if bytes.get(j) == Some(&b'/') {
            j += 1;
        } else {
            while bytes.get(j) == Some(&b'\\') {
                j += 1;
            }
        }
        if j == at + 5 {
            continue;
        }
        let Some(len) = names.iter().find_map(|n| {
            let len = name_len(&s[j..], n)?;
            ends_name(&s[j + len..]).then_some(len)
        }) else {
            continue;
        };
        out.push_str(&s[copied..j]);
        out.push_str("<usuario>");
        copied = j + len;
        from = copied;
    }
    out.push_str(&s[copied..]);
    out
}

/// Whether a name (`token`, `X-Auth`, `passwordhash`…) holds a value that must never be shown.
/// A name that merely contains one of the words counts, so `oauth` and `user_password_hash`
/// are hidden too.
fn secret_key(key: &str) -> bool {
    let k = key.trim_start_matches('-').to_ascii_lowercase();
    [
        "token",
        "password",
        "passwd",
        "secret",
        "auth",
        "cookie",
        "loginsecure",
    ]
    .iter()
    .any(|w| k.contains(w))
        || matches!(k.as_str(), "steamid" | "sessionid" | "apikey" | "api_key")
}

/// Whether a `name: value` pair is a secret even when the name is not quoted like a JSON key.
fn secret_key_before_colon(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    [
        "token",
        "password",
        "passwd",
        "secret",
        "cookie",
        "loginsecure",
    ]
    .iter()
    .any(|w| k.contains(w))
        || k == "authorization"
}

/// Names whose value must never be shown: `token=…`, `access_token="…"`, `password=…`,
/// `-steamid=…`, `auth=…`, `{"password":"…"}`, `Authorization: Bearer …` and the like. A quoted
/// value goes up to its closing quote; any other value goes up to the next space, quote or `&`.
fn hide_secret_values(s: &str) -> String {
    let bytes = s.as_bytes();
    let is_key = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let is_quote = |b: u8| matches!(b, b'"' | b'\'' | b'\\');
    let mut out = String::with_capacity(s.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        let sep = bytes[i];
        if sep != b'=' && sep != b':' {
            i += 1;
            continue;
        }
        // The name before the separator; a JSON key has its closing quote in between.
        let mut key_end = i;
        while key_end > copied && is_quote(bytes[key_end - 1]) {
            key_end -= 1;
        }
        let quoted_key = key_end < i;
        let mut key_start = key_end;
        while key_start > copied && is_key(bytes[key_start - 1]) {
            key_start -= 1;
        }
        let key = &s[key_start..key_end];
        let secret = match sep {
            b'=' => key_end == i && secret_key(key),
            _ => {
                !key.is_empty() && ((quoted_key && secret_key(key)) || secret_key_before_colon(key))
            }
        };
        if !secret {
            i += 1;
            continue;
        }
        // The value: after spaces (colon form only) and opening quotes.
        let mut v = i + 1;
        if sep == b':' {
            while v < bytes.len() && (bytes[v] == b' ' || bytes[v] == b'\t') {
                v += 1;
            }
        }
        let quoted_value = v < bytes.len() && is_quote(bytes[v]);
        while v < bytes.len() && is_quote(bytes[v]) {
            v += 1;
        }
        let ends: &[char] = if sep == b':' {
            &['"', '\'', '&', ',', '}', ';']
        } else {
            &['"', '\'', '&']
        };
        let mut end = s[v..]
            .find(|c: char| c.is_whitespace() || ends.contains(&c))
            .map_or(s.len(), |n| v + n);
        // A quoted value may hold spaces: it goes up to the closing quote.
        if quoted_value {
            end = s[v..].find(['"', '\'', '\\']).map_or(s.len(), |n| v + n);
        }
        // `Authorization: Bearer <token>`: the scheme word is not the secret, the next one is.
        let scheme = s[v..end].to_ascii_lowercase();
        if sep == b':' && !quoted_value && (scheme == "bearer" || scheme == "basic") {
            let rest = &s[end..];
            let skipped = rest.len() - rest.trim_start().len();
            let word = rest[skipped..]
                .find(char::is_whitespace)
                .unwrap_or(rest.len() - skipped);
            end += skipped + word;
        }
        if end == v {
            i += 1;
            continue;
        }
        out.push_str(&s[copied..v]);
        out.push_str("<oculto>");
        copied = end;
        i = end;
    }
    out.push_str(&s[copied..]);
    out
}

/// The 32-bit Steam account number: the digits after `userdata/` or `userdata\\`, in `[U:1:n]`
/// and in `STEAM_x:y:n`. It identifies the account just like the 17-digit form does.
fn hide_short_steam_ids(s: &str) -> String {
    // ASCII lower-casing keeps every byte position, so indexes into `low` fit `s`.
    let low = s.to_ascii_lowercase();
    let bytes = low.as_bytes();
    let digits_at = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut out = String::with_capacity(s.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        let rest = &bytes[i..];
        // Where the number would start, if this is one of the three forms.
        let start = if rest.starts_with(b"userdata") {
            let mut j = i + 8;
            if bytes.get(j) == Some(&b'/') {
                j += 1;
            } else {
                while bytes.get(j) == Some(&b'\\') {
                    j += 1;
                }
            }
            (j > i + 8).then_some(j)
        } else if rest.starts_with(b"[u:") {
            // `[U:1:n]`: the universe digit, a colon, then the number.
            let j = i + 3;
            let n = digits_at(j);
            (n > 0 && bytes.get(j + n) == Some(&b':')).then_some(j + n + 1)
        } else if rest.starts_with(b"steam_") {
            // `STEAM_x:y:n`.
            let j = i + 6;
            let a = digits_at(j);
            let b = if a > 0 && bytes.get(j + a) == Some(&b':') {
                digits_at(j + a + 1)
            } else {
                0
            };
            let k = j + a + 1;
            (b > 0 && bytes.get(k + b) == Some(&b':')).then_some(k + b + 1)
        } else {
            None
        };
        let Some(start) = start else {
            i += 1;
            continue;
        };
        let n = digits_at(start);
        if n == 0 {
            i += 1;
            continue;
        }
        out.push_str(&s[copied..start]);
        out.push_str("<id-steam>");
        copied = start + n;
        i = copied;
    }
    out.push_str(&s[copied..]);
    out
}

/// Steam account numbers: 17 digits that start with 7656119.
fn hide_steam_ids(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut digits = String::new();
    let flush = |digits: &mut String, out: &mut String| {
        if digits.len() == 17 && digits.starts_with("7656119") {
            out.push_str("<id-steam>");
        } else {
            out.push_str(digits);
        }
        digits.clear();
    };
    for c in s.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            flush(&mut digits, &mut out);
            out.push(c);
        }
    }
    flush(&mut digits, &mut out);
    out
}

fn is_email(w: &str) -> bool {
    match w.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain
                    .split_once('.')
                    .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty())
        }
        None => false,
    }
}

/// A JSON Web Token: three dot-separated parts, the first starting with `eyJ`.
fn is_jwt(w: &str) -> bool {
    w.starts_with("eyJ")
        && w.matches('.').count() == 2
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// Looks at each word (split on spaces, tabs, quotes, commas, colons, semicolons and `=`) and
/// hides e-mail addresses, tokens and the launchers' private arguments.
fn hide_words(s: &str) -> String {
    let is_separator =
        |c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ':' | ';' | '=');
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    let flush = |word: &mut String, next: Option<char>, out: &mut String| {
        if word.is_empty() {
            return;
        }
        let lw = word.to_lowercase();
        let hidden = if is_email(word) {
            Some("<correo>")
        } else if lw.contains("rline_")
            || lw.contains("branchaccess")
            // `password=<oculto>` already shows only the name; a lone word is hidden.
            || (lw.contains("password") && next != Some('='))
            || is_jwt(word)
            || (word.len() >= 32
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+/-_".contains(c)))
        {
            Some("<oculto>")
        } else {
            None
        };
        out.push_str(hidden.unwrap_or(word));
        word.clear();
    };
    for c in s.chars() {
        if is_separator(c) {
            flush(&mut word, Some(c), &mut out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, None, &mut out);
    out
}
