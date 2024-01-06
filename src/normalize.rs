pub fn trim<S: AsRef<[u8]>>(output: S) -> String {
    let bytes = output.as_ref();
    let mut normalized = String::from_utf8_lossy(bytes).into_owned();

    let len = normalized.trim_end().len();
    normalized.truncate(len);

    if !normalized.is_empty() {
        normalized.push('\n');
    }

    normalized
}
