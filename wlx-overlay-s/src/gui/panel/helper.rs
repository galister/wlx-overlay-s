use regex::Regex;
use std::{
    io::{BufRead, BufReader, Read},
    sync::LazyLock,
};

static ENV_VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)}|\$([A-Z_][A-Z0-9_]*)").unwrap() // want panic
});

pub(super) fn expand_env_vars(template: &str) -> String {
    ENV_VAR_REGEX
        .replace_all(template, |caps: &regex::Captures| {
            let var_name = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str();
            std::env::var(var_name)
                .inspect_err(|e| log::warn!("Unable to substitute env var {var_name}: {e:?}"))
                .unwrap_or_default()
        })
        .into_owned()
}

pub(super) fn read_label_from_pipe<R>(
    path: &str,
    reader: &mut BufReader<R>,
    carry_over: &mut Option<String>,
) -> Option<String>
where
    R: Read + Sized,
{
    let mut prev = String::new();
    let mut cur = String::new();

    for r in reader.lines() {
        match r {
            Ok(line) => {
                prev = cur;
                cur = carry_over.take().map(|s| s + &line).unwrap_or(line);
            }
            Err(e) => {
                log::warn!("pipe read error on {path}: {e:?}");
                return None;
            }
        }
    }

    carry_over.replace(cur);

    if prev.is_empty() { None } else { Some(prev) }
}
