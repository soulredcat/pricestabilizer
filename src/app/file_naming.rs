//! File names derived from feed names, which may themselves be paths.

pub fn feed_file_name(feed_name: &str) -> String {
    feed_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::feed_file_name;

    #[test]
    fn paths_become_flat_names() {
        assert_eq!(feed_file_name("data/feed.jsonl"), "data_feed_jsonl");
        assert_eq!(feed_file_name("stdin"), "stdin");
    }
}
